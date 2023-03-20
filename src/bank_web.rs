use axum::{
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::bank::accounts::AccountService;

mod payments;
mod refunds;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ErrorResponseBody {
    error: String,
}
impl ErrorResponseBody {
    pub fn new(s: &'static str) -> Self {
        Self {
            error: s.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct BankWeb<T> {
    pool: PgPool,
    #[allow(dead_code)]
    account_service: T,
}

impl<T: AccountService> BankWeb<T> {
    pub fn new(pool: PgPool, account_service: T) -> Self {
        Self {
            pool,
            account_service,
        }
    }

    pub fn into_router(self) -> Router {
        Router::new()
            .route("/api/payments", post(payments::post::<T>))
            .route("/api/payments/:payment_id", get(payments::get::<T>))
            .route(
                "/api/payments/:payment_id/refunds",
                post(refunds::post::<T>),
            )
            .route(
                "/api/payments/:payment_id/refunds/:refund_id",
                get(refunds::get::<T>),
            )
            .layer(axum_tracing_opentelemetry::opentelemetry_tracing_layer())
            .with_state(self)
            .with_state(())
    }
}

#[cfg(test)]
pub mod tests {
    use axum::{
        body::Bytes,
        http::{header::CONTENT_TYPE, Method, Request},
    };
    use http_body::combinators::UnsyncBoxBody;
    use serde::{de::DeserializeOwned, Serialize};
    use tower::ServiceExt;

    use super::*;
    use crate::bank::accounts::DummyService;

    impl BankWeb<DummyService> {
        pub async fn new_test() -> Self {
            Self {
                pool: crate::pg_pool()
                    .await
                    .expect("failed to create postgres pool"),
                account_service: DummyService::default(),
            }
        }

        pub async fn new_test_with_response(response: impl Into<String>) -> Self {
            let mut bank_web = Self::new_test().await;
            bank_web.account_service.response = Some(response.into());
            bank_web
        }
    }

    pub async fn send_request(
        router: &Router,
        request: Request<hyper::Body>,
    ) -> hyper::Response<UnsyncBoxBody<Bytes, axum::Error>> {
        router
            .clone()
            .oneshot(request)
            .await
            .expect("failed to send oneshot request")
    }

    pub async fn get(
        router: &Router,
        uri: impl AsRef<str>,
    ) -> hyper::Response<UnsyncBoxBody<Bytes, axum::Error>> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri.as_ref())
            .body(hyper::Body::empty())
            .expect("failed to build GET request");
        send_request(router, request).await
    }

    pub async fn post<T: Serialize>(
        router: &Router,
        uri: impl AsRef<str>,
        body: &T,
    ) -> hyper::Response<UnsyncBoxBody<Bytes, axum::Error>> {
        let request = Request::builder()
            .method(Method::POST)
            .uri(uri.as_ref())
            .header(CONTENT_TYPE, "application/json")
            .body(
                serde_json::to_vec(body)
                    .expect("failed to serialize POST body")
                    .into(),
            )
            .expect("failed to build POST request");
        send_request(router, request).await
    }

    pub async fn deserialize_response_body<T>(
        response: hyper::Response<UnsyncBoxBody<Bytes, axum::Error>>,
    ) -> T
    where
        T: DeserializeOwned,
    {
        let bytes = hyper::body::to_bytes(response.into_body())
            .await
            .expect("failed to read response body into bytes");
        serde_json::from_slice::<T>(&bytes).expect("failed to deserialize response")
    }
}
