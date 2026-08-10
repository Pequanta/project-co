//! Minimal, typed subset of the Telegram Bot API needed by this bot.
//! Field names follow the Bot API (snake_case on the wire).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    #[allow(dead_code)]
    pub message_id: i64,
    pub chat: Chat,
    #[serde(default)]
    pub from: Option<User>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub id: i64,
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    #[serde(default)]
    pub message: Option<CallbackMessage>,
    #[serde(default)]
    pub data: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackMessage {
    #[allow(dead_code)]
    pub message_id: i64,
    pub chat: Chat,
}

// --- Outbound parameters -------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

impl InlineKeyboardMarkup {
    pub fn single_row(rows: Vec<(String, String)>) -> Self {
        InlineKeyboardMarkup {
            inline_keyboard: rows
                .into_iter()
                .map(|(text, callback_data)| {
                    vec![InlineKeyboardButton {
                        text,
                        callback_data: Some(callback_data),
                    }]
                })
                .collect(),
        }
    }

    pub fn two_columns(rows: Vec<Vec<(String, String)>>) -> Self {
        InlineKeyboardMarkup {
            inline_keyboard: rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|(text, callback_data)| InlineKeyboardButton {
                            text,
                            callback_data: Some(callback_data),
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SendMessageParams<'a> {
    pub chat_id: i64,
    pub text: &'a str,
    pub parse_mode: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct EditMessageParams<'a> {
    pub chat_id: i64,
    pub message_id: i64,
    pub text: &'a str,
    pub parse_mode: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnswerCallbackParams<'a> {
    pub callback_query_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetWebhookParams<'a> {
    pub url: &'a str,
    pub secret_token: &'a str,
    pub allowed_updates: Vec<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageResult {
    #[allow(dead_code)]
    pub message_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
    pub error_code: Option<i64>,
}
