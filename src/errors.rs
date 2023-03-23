use axum::http::StatusCode;
use std::fmt::Display;

use crate::bank::payments::Status;

#[derive(Debug)]
pub struct PaymentError {
    pub code: i32,
    pub message: String,
}

impl Display for PaymentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for PaymentError {
    fn default() -> Self {
        PaymentError {
            code: 403,
            message: "Forbidden".to_string(),
        }
    }
}

impl PaymentError {
    pub fn from(messages: &str) -> PaymentError {
        let (code, message) = match messages {
            "invalid_account_number" => (403, "Forbidden"),
            "invalid_amount" => (400, "Bad Request"),
            "insufficient_funds" => (402, "Payment Required"),
            "service_unavailable" => (503, "Service unavailable"),
            _ => (500, "Internal Error"),
        };
        PaymentError {
            code,
            message: message.to_string(),
        }
    }

    pub fn get_payment_status(&self) -> Status {
        match self.code {
            402 | 403 => Status::Declined,
            _ => Status::Failed,
        }
    }

    pub fn get_http_status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.code as u16).unwrap_or(StatusCode::NOT_FOUND)
    }
}
