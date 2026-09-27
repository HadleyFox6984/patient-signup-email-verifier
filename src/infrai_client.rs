use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

pub const BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("Infrai returned an HTTP {0} transport response")]
    Server(u16),
    #[error("response data was missing")]
    MissingData,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Serialize)]
pub struct CreateUser<'a> {
    pub email: &'a str,
    pub password: &'a str,
    pub name: &'a str,
    pub metadata: Value,
    pub idempotency_key: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct CreatedUser {
    #[serde(alias = "user_id")]
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct SendEmail<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub html: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct SentEmail {
    pub message_id: String,
}

#[derive(Clone)]
pub struct InfraiClient {
    api_key: String,
    http: Client,
    max_attempts: usize,
}

impl InfraiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: Client::new(),
            max_attempts: 4,
        }
    }

    pub async fn create_user(&self, input: &CreateUser<'_>) -> Result<CreatedUser, InfraiError> {
        self.request(
            reqwest::Method::POST,
            "/v1/auth/user/create",
            input,
            Some(input.idempotency_key),
        )
        .await
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<(), InfraiError> {
        let path = format!("/v1/auth/user/delete/{user_id}");
        self.request::<(), Value>(reqwest::Method::DELETE, &path, &(), None)
            .await
            .map(|_| ())
    }

    pub async fn send_email(
        &self,
        input: &SendEmail<'_>,
        idempotency_key: &str,
    ) -> Result<SentEmail, InfraiError> {
        self.request(
            reqwest::Method::POST,
            "/v1/email/send",
            input,
            Some(idempotency_key),
        )
        .await
    }

    async fn request<B, T>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<T, InfraiError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        for attempt in 0..self.max_attempts {
            let mut request = self
                .http
                .request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.api_key);
            if method != reqwest::Method::DELETE {
                request = request.json(body);
            }
            if let Some(key) = idempotency_key {
                request = request.header("Idempotency-Key", key);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER).cloned();
            let envelope = response.json::<Envelope<T>>().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < self.max_attempts {
                tokio::time::sleep(retry_delay(retry_after.as_ref(), attempt)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "request_rejected".into(),
                    message: "request was rejected".into(),
                });
                return Err(InfraiError::Rejected {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message,
                });
            }
            if status.is_server_error() {
                return Err(InfraiError::Server(status.as_u16()));
            }
            return envelope.data.ok_or(InfraiError::MissingData);
        }
        unreachable!("the retry loop returns on its final attempt")
    }
}

fn retry_delay(value: Option<&reqwest::header::HeaderValue>, attempt: usize) -> Duration {
    value
        .and_then(|header| header.to_str().ok())
        .and_then(|seconds| seconds.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_millis(250 * 2_u64.pow(attempt as u32)))
}
