use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{BankWeb, ErrorResponseBody};
use crate::bank::{
    accounts::{AccountService, HoldRef},
    payment_instruments::Card,
    payments::{self, Status},
};
use crate::errors::PaymentError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RequestData {
    pub amount: i32,
    pub card_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RequestBody {
    pub payment: RequestData,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ResponseData {
    pub id: Uuid,
    pub amount: i32,
    pub card_number: String,
    pub status: payments::Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ResponseBody {
    pub data: ResponseData,
}
impl ResponseBody {
    pub fn new(id: Uuid, amount: i32, card_number: String, status: Status) -> Self {
        ResponseBody {
            data: ResponseData {
                id,
                amount,
                card_number,
                status,
            },
        }
    }
}

macro_rules! unwrap_or_return {
    ( $res:expr, $err:expr ) => {
        match $res {
            Ok(x) => x,
            Err(_) => return $err,
        }
    };
}

macro_rules! check_and_reverse_payment_status {
    ($bank_web:ident, $payment_result:ident, $payment_id:ident, $card_number:ident, $amount:ident ) => {
        if let Err(err_str) = $payment_result {
            let payment_err = PaymentError::from(&err_str);
            // update payment status to Declined or Failed, according to the payment_err type
            payments::update(
                &$bank_web.pool,
                $payment_id,
                payment_err.get_payment_status(),
            )
            .await
            .unwrap();
            return Ok((
                payment_err.get_http_status_code(),
                Json(ResponseBody::new(
                    Uuid::new_v4(),
                    $amount,
                    $card_number,
                    payment_err.get_payment_status(),
                )),
            ));
        }
    };
}

pub async fn post<T: AccountService>(
    State(bank_web): State<BankWeb<T>>,
    Json(body): Json<RequestBody>,
) -> Result<(StatusCode, Json<ResponseBody>), (StatusCode, Json<ErrorResponseBody>)> {
    let amount = body.payment.amount;
    let card_number = body.payment.card_number.to_string();

    // payment requests for 0 should return a 204 response
    if amount == 0 {
        return Err((
            StatusCode::NO_CONTENT,
            Json(ErrorResponseBody::new("Amount shouldn't be 0")),
        ));
    }

    // payment requests for negative amounts should return a 400 response
    if amount < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponseBody::new("Amount shouldn't be negative")),
        ));
    }

    // invalid card formats should return a 422 response
    let card = match Card::try_from(card_number.clone()) {
        Ok(c) => c,
        Err(_e) => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponseBody::new("Bad Card Number format")),
            ))
        }
    };

    // insert Processing Payment
    let payment_id = unwrap_or_return!(
        payments::insert(
            &bank_web.pool,
            body.payment.amount,
            body.payment.card_number,
            payments::Status::Processing
        )
        .await,
        Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponseBody::new("card_number already used")),
        ))
    );
    // place hold
    let payment_result = bank_web
        .account_service
        .place_hold(card.account_number(), body.payment.amount)
        .await;

    // deal with payment_result
    check_and_reverse_payment_status!(bank_web, payment_result, payment_id, card_number, amount);

    payments::update(&bank_web.pool, payment_id, payments::Status::Approved)
        .await
        .unwrap();
    let payment_result = bank_web
        .account_service
        .withdraw_funds(payment_result.unwrap())
        .await;

    // deal with payment_result
    check_and_reverse_payment_status!(bank_web, payment_result, payment_id, card_number, amount);

    Ok((
        StatusCode::CREATED,
        Json(ResponseBody::new(
            payment_id,
            amount,
            card_number,
            payments::Status::Approved,
        )),
    ))
}

pub async fn get<T: AccountService>(
    State(bank_web): State<BankWeb<T>>,
    Path(payment_id): Path<Uuid>,
) -> Result<(StatusCode, Json<ResponseBody>), (StatusCode, Json<ErrorResponseBody>)> {
    let payment = payments::get(&bank_web.pool, payment_id).await.unwrap();

    Ok((
        StatusCode::OK,
        Json(ResponseBody {
            data: ResponseData {
                id: payment.id,
                amount: payment.amount,
                card_number: payment.card_number,
                status: payment.status,
            },
        }),
    ))
}

#[cfg(test)]
pub mod tests {

    use super::*;
    use crate::bank::accounts::{AccountService, DummyService, HoldRef};
    use crate::{
        bank::{payment_instruments::Card, payments::Status},
        bank_web::tests::{deserialize_response_body, get, post},
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Clone, Default)]
    struct MockService {
        dummy: DummyService,
        place_hold_count: Arc<AtomicUsize>,
        release_hold_count: Arc<AtomicUsize>,
        withdraw_funds_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AccountService for MockService {
        async fn place_hold(&self, account_number: &str, amount: i32) -> Result<HoldRef, String> {
            self.place_hold_count.fetch_add(1, Ordering::SeqCst);
            self.dummy.place_hold(account_number, amount).await
        }

        async fn release_hold(&self, hold_ref: HoldRef) -> Result<(), String> {
            self.release_hold_count.fetch_add(1, Ordering::SeqCst);
            self.dummy.release_hold(hold_ref).await
        }

        async fn withdraw_funds(&self, hold_ref: HoldRef) -> Result<(), String> {
            self.withdraw_funds_count.fetch_add(1, Ordering::SeqCst);
            self.dummy.withdraw_funds(hold_ref).await
        }
    }

    #[tokio::test]
    async fn should_not_place_hold_for_payment_with_negative_amount() {
        let pool = crate::pg_pool().await.unwrap();
        let mock_service = MockService::default();
        let router = BankWeb::new(pool, mock_service.clone()).into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: -1,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 400);
        assert_eq!(
            mock_service.place_hold_count.load(Ordering::SeqCst),
            0,
            "should not try to place hold for amount -1"
        );
    }

    #[tokio::test]
    async fn should_withdraw_funds_on_successful_payment() {
        let pool = crate::pg_pool().await.unwrap();
        let mock_service = MockService::default();
        let router = BankWeb::new(pool, mock_service.clone()).into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: 123,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 201);
        assert_eq!(mock_service.withdraw_funds_count.load(Ordering::SeqCst), 1);
    }

    async fn make_payment(router: axum::Router, card: Card) -> hyper::StatusCode {
        let request_body = RequestBody {
            payment: RequestData {
                amount: 123,
                card_number: card.into(),
            },
        };
        let response = post(&router, "/api/payments", &request_body).await;
        response.status()
    }

    #[tokio::test]
    async fn should_not_place_holds_for_concurrent_payments() {
        let pool = crate::pg_pool().await.unwrap();
        let mock_service = MockService::default();
        let router = BankWeb::new(pool, mock_service.clone()).into_router();

        let card = Card::new_test();

        let fut_a = make_payment(router.clone(), card.clone());
        let fut_b = make_payment(router, card.clone());
        let (status_a, status_b) = tokio::join!(fut_a, fut_b);

        assert_eq!(status_a.min(status_b), 201, "one payment should succeed");
        assert_eq!(status_a.max(status_b), 422, "one payment should fail");

        assert_eq!(
            mock_service.place_hold_count.load(Ordering::SeqCst),
            1,
            "should not try to place hold for concurrent requests"
        );
    }

    #[tokio::test]
    async fn should_approve_valid_payment() {
        let router = BankWeb::new_test().await.into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: 1205,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 201);

        let response_body = deserialize_response_body::<ResponseBody>(response).await;
        assert_eq!(response_body.data.amount, request_body.payment.amount);

        let uri = format!("/api/payments/{}", response_body.data.id);
        let response = get(&router, uri).await;
        assert_eq!(response.status(), 200);

        let response_body = deserialize_response_body::<ResponseBody>(response).await;
        assert_eq!(response_body.data.amount, request_body.payment.amount);
        assert_eq!(response_body.data.status, Status::Approved);
    }

    #[tokio::test]
    async fn should_decline_payment_and_return_402_with_insufficient_funds() {
        let router = BankWeb::new_test_with_response("insufficient_funds")
            .await
            .into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: 1205,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 402);

        let response_body = deserialize_response_body::<ResponseBody>(response).await;
        assert_eq!(response_body.data.amount, request_body.payment.amount);
        assert_eq!(response_body.data.status, Status::Declined);
    }

    #[tokio::test]
    async fn should_decline_payment_and_return_403_for_invalid_account_number() {
        let router = BankWeb::new_test_with_response("invalid_account_number")
            .await
            .into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: 1205,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 403);

        let response_body = deserialize_response_body::<ResponseBody>(response).await;
        assert_eq!(response_body.data.amount, request_body.payment.amount);
        assert_eq!(response_body.data.status, Status::Declined);
    }

    #[tokio::test]
    async fn should_return_204_for_zero_amount() {
        let router = BankWeb::new_test().await.into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: 0,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 204);
    }

    #[tokio::test]
    async fn should_return_422_for_existing_card_number() {
        let router = BankWeb::new_test().await.into_router();

        let request_body = RequestBody {
            payment: RequestData {
                amount: 123,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 201);

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 422);

        let response_body = deserialize_response_body::<ErrorResponseBody>(response).await;
        assert_eq!(response_body.error, "card_number already used");
    }
}
