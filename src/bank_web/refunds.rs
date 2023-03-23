use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{BankWeb, ErrorResponseBody};
use crate::bank::{accounts::AccountService, payments::Status, refunds};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestData {
    amount: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestBody {
    refund: RequestData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseData {
    id: Uuid,
    amount: i32,
    payment_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseBody {
    data: ResponseData,
}

impl ResponseBody {
    pub fn new(id: Uuid, amount: i32, payment_id: Uuid) -> Self {
        Self {
            data: ResponseData {
                id,
                amount,
                payment_id,
            },
        }
    }
}

macro_rules! unwrap_or_return {
    ( $e:expr, $err:expr ) => {
        match $e {
            Ok(x) => x,
            Err(_) => return $err,
        }
    };
}

pub async fn post<T: AccountService>(
    State(bank_web): State<BankWeb<T>>,
    Path(payment_id): Path<Uuid>,
    Json(body): Json<RequestBody>,
) -> Result<(StatusCode, Json<ResponseBody>), (StatusCode, Json<ErrorResponseBody>)> {
    // body.refund.amount

    // Gettting the payment details from payment table
    let payment_result = crate::bank::payments::get(&bank_web.pool, payment_id)
        .await
        .ok();

    if let Some(p) = payment_result {
        if p.status != Status::Approved {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponseBody::new("has a status other than approved")),
            ));
        }
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponseBody::new("payment doesn't exist")),
        ));
    };

    let refund_id = unwrap_or_return!(
        refunds::checked_insert(&bank_web.pool, payment_id, body.refund.amount).await,
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponseBody::new(
                "can't add refund since the db problem"
            )),
        ))
    );

    if refund_id.is_none() {
        Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponseBody::new("excessive refund amount requested")),
        ))
    } else {
        Ok((
            StatusCode::CREATED,
            Json(ResponseBody::new(
                refund_id.unwrap(),
                body.refund.amount,
                payment_id,
            )),
        ))
    }
}

pub async fn get<T: AccountService>(
    State(bank_web): State<BankWeb<T>>,
    Path((payment_id, refund_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<ResponseBody>), (StatusCode, Json<ErrorResponseBody>)> {
    let data = refunds::get(&bank_web.pool, refund_id).await.unwrap();

    Ok((
        StatusCode::OK,
        Json(ResponseBody::new(data.id, data.amount, payment_id)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bank::{payment_instruments::Card, payments::Status},
        bank_web::{
            payments,
            tests::{deserialize_response_body, get, post},
        },
    };

    async fn setup() -> (axum::Router, payments::ResponseBody) {
        let router = BankWeb::new_test().await.into_router();

        let request_body = payments::RequestBody {
            payment: payments::RequestData {
                amount: 1205,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 201);

        let response_body = deserialize_response_body::<payments::ResponseBody>(response).await;
        assert_eq!(response_body.data.status, Status::Approved);

        (router, response_body)
    }

    async fn request_refund(router: axum::Router, payment_id: Uuid) -> StatusCode {
        let request_body = RequestBody {
            refund: RequestData { amount: 1205 },
        };

        let uri = format!("/api/payments/{payment_id}/refunds",);
        let response = post(&router, uri, &request_body).await;
        response.status()
    }

    #[tokio::test]
    async fn should_handle_concurrent_refunds() {
        let router = BankWeb::new_test().await.into_router();

        let request_body = payments::RequestBody {
            payment: payments::RequestData {
                amount: 1205,
                card_number: Card::new_test().into(),
            },
        };

        let response = post(&router, "/api/payments", &request_body).await;
        assert_eq!(response.status(), 201);

        let response_body = deserialize_response_body::<payments::ResponseBody>(response).await;
        let payment_id = response_body.data.id;

        let fut_a = request_refund(router.clone(), payment_id);
        let fut_b = request_refund(router, payment_id);
        let (status_a, status_b) = tokio::join!(fut_a, fut_b);

        assert_eq!(status_a.min(status_b), 201, "one refund should succeed");
        assert_eq!(status_a.max(status_b), 422, "one refund should fail");
    }

    #[tokio::test]
    async fn should_refund_valid_amount() {
        let (router, payment_response_body) = setup().await;
        let payment_id = payment_response_body.data.id;

        let request_body = RequestBody {
            refund: RequestData { amount: 42 },
        };

        let uri = format!("/api/payments/{payment_id}/refunds",);
        let response = post(&router, uri.to_string(), &request_body).await;
        assert_eq!(response.status(), 201);

        let response_body = deserialize_response_body::<ResponseBody>(response).await;
        assert_eq!(response_body.data.amount, request_body.refund.amount);
        let refund_id = response_body.data.id;

        let uri = format!("/api/payments/{payment_id}/refunds/{refund_id}");
        let response = get(&router, uri).await;
        assert_eq!(response.status(), 200);

        let response_body = deserialize_response_body::<ResponseBody>(response).await;
        assert_eq!(response_body.data.amount, request_body.refund.amount);
    }

    #[tokio::test]
    async fn should_reject_refund_of_invalid_amount() {
        let (router, payment_response_body) = setup().await;
        let payment_id = payment_response_body.data.id;

        let request_body = RequestBody {
            refund: RequestData {
                amount: payment_response_body.data.amount + 1,
            },
        };

        let uri = format!("/api/payments/{payment_id}/refunds",);
        let response = post(&router, uri, &request_body).await;
        assert_eq!(response.status(), 422);

        let response_body = deserialize_response_body::<ErrorResponseBody>(response).await;
        assert_eq!(response_body.error, "excessive refund amount requested");
    }
}
