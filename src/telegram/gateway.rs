//! Telegram outbound gateway. This is the only place that talks to the Bot
//! API — application and notification services depend on the `TelegramGateway`
//! trait, so the transport is replaceable (and mockable in tests).

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::telegram::types::{
    ApiResponse, EditMessageParams, InlineKeyboardMarkup, SendMessageParams, SendMessageResult,
    SetWebhookParams,
};

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("telegram api error {code}: {description}")]
    Api {
        code: i64,
        description: String,
        retry_after: Option<u64>,
    },
    #[error("telegram transport error: {0}")]
    Transport(String),
    #[error("unexpected telegram response: {0}")]
    Unexpected(String),
}

#[async_trait]
pub trait TelegramGateway: Send + Sync {
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<SendMessageResult, GatewayError>;

    #[allow(dead_code)]
    async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<(), GatewayError>;

    async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<(), GatewayError>;

    async fn set_webhook(&self, url: &str, secret_token: &str) -> Result<(), GatewayError>;
}

/// Production gateway backed by the real Bot API over HTTPS.
pub struct ReqwestGateway {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl ReqwestGateway {
    pub fn new(token: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("project-co-bot/0.1")
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            base_url: "https://api.telegram.org".to_string(),
            token,
        }
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: &(impl serde::Serialize + Sync),
    ) -> Result<T, GatewayError> {
        let url = format!("{}/bot{}/{}", self.base_url, self.token, method);
        let mut attempts = 0;
        loop {
            attempts += 1;
            let resp = self
                .http
                .post(&url)
                .json(params)
                .send()
                .await
                .map_err(|e| GatewayError::Transport(e.to_string()))?;
            let status = resp.status();
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let body = resp
                .text()
                .await
                .map_err(|e| GatewayError::Transport(e.to_string()))?;

            let api: ApiResponse<T> = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    return Err(GatewayError::Unexpected(format!(
                        "status={status} body={body:?} parse={e}"
                    )))
                }
            };
            if api.ok {
                return api.result.ok_or_else(|| {
                    GatewayError::Unexpected(format!("ok=true but no result for {method}"))
                });
            }

            let code = api.error_code.unwrap_or(0);
            // One retry on 429 with backoff; otherwise fail.
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempts < 2 {
                let wait = retry_after.unwrap_or(1).min(5);
                tracing::warn!(wait_s = wait, "telegram rate limited, retrying");
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                continue;
            }
            return Err(GatewayError::Api {
                code,
                description: api.description.unwrap_or_else(|| body.clone()),
                retry_after,
            });
        }
    }
}

#[async_trait]
impl TelegramGateway for ReqwestGateway {
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<SendMessageResult, GatewayError> {
        let params = SendMessageParams {
            chat_id,
            text,
            parse_mode: "Markdown",
            reply_markup,
        };
        self.call("sendMessage", &params).await
    }

    async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<(), GatewayError> {
        let params = EditMessageParams {
            chat_id,
            message_id,
            text,
            parse_mode: "Markdown",
            reply_markup,
        };
        let _: SendMessageResult = self.call("editMessageText", &params).await?;
        Ok(())
    }

    async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<(), GatewayError> {
        self.call(
            "answerCallbackQuery",
            &crate::telegram::types::AnswerCallbackParams {
                callback_query_id,
                text,
            },
        )
        .await
        .map(|_: serde_json::Value| ())
    }

    async fn set_webhook(&self, url: &str, secret_token: &str) -> Result<(), GatewayError> {
        let params = SetWebhookParams {
            url,
            secret_token,
            allowed_updates: vec!["message", "callback_query"],
        };
        self.call("setWebhook", &params)
            .await
            .map(|_: serde_json::Value| ())
    }
}

#[cfg(test)]
pub mod test_gateway {
    //! In-memory gateway for tests: records sent messages.
    use super::*;
    use std::sync::Mutex;

    pub struct RecordingGateway {
        pub sent: Mutex<Vec<(i64, String)>>,
    }

    impl Default for RecordingGateway {
        fn default() -> Self {
            Self {
                sent: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl TelegramGateway for RecordingGateway {
        async fn send_message(
            &self,
            chat_id: i64,
            text: &str,
            _reply_markup: Option<InlineKeyboardMarkup>,
        ) -> Result<SendMessageResult, GatewayError> {
            self.sent.lock().unwrap().push((chat_id, text.to_string()));
            Ok(SendMessageResult { message_id: 1 })
        }

        async fn edit_message_text(
            &self,
            _chat_id: i64,
            _message_id: i64,
            _text: &str,
            _reply_markup: Option<InlineKeyboardMarkup>,
        ) -> Result<(), GatewayError> {
            Ok(())
        }

        async fn answer_callback_query(
            &self,
            _callback_query_id: &str,
            _text: Option<&str>,
        ) -> Result<(), GatewayError> {
            Ok(())
        }

        async fn set_webhook(&self, _url: &str, _secret_token: &str) -> Result<(), GatewayError> {
            Ok(())
        }
    }
}
