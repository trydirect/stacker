use crate::db;
use crate::helpers::{self, JsonResponse};
use actix_web::{get, web, Responder, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

const MAX_LIMIT: i64 = 50;
const MESSAGES_PER_CHAT: usize = 5;
const MAX_CONTENT_CHARS: usize = 500;

#[derive(Debug, Deserialize)]
pub struct Query {
    pub limit: Option<i64>,
}

fn truncate_content(message: &Value) -> Value {
    let mut message = message.clone();
    if let Some(content) = message.get("content").and_then(Value::as_str) {
        if content.chars().count() > MAX_CONTENT_CHARS {
            let cut: String = content.chars().take(MAX_CONTENT_CHARS).collect();
            message["content"] = Value::String(format!("{cut}…"));
        }
    }
    message
}

fn last_messages(messages: &Value) -> (usize, Vec<Value>) {
    match messages.as_array() {
        Some(all) => (
            all.len(),
            all.iter()
                .skip(all.len().saturating_sub(MESSAGES_PER_CHAT))
                .map(truncate_content)
                .collect(),
        ),
        None => (0, vec![]),
    }
}

/// GET /api/admin/chat/recent[?limit=N]
/// Admin-only. Returns the last messages of the most recently active chats,
/// restricted to users who accepted the `ai_chat_analysis` agreement.
#[tracing::instrument(name = "Admin recent consented chats.", skip_all)]
#[get("/recent")]
pub async fn recent(
    query: web::Query<Query>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let limit = query.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
    let chats = db::chat_admin::recent_consented(pg_pool.get_ref(), limit)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;

    let list: Vec<Value> = chats
        .into_iter()
        .map(|chat| {
            let messages = match chat.plain_messages {
                Some(plain) => plain,
                None => helpers::chat::decrypt_messages(&chat.messages_encrypted)
                    .unwrap_or_else(|err| {
                        tracing::warn!("admin chat decrypt failed for {}: {}", chat.id, err);
                        Value::Array(vec![])
                    }),
            };
            let (message_count, last) = last_messages(&messages);
            json!({
                "source": chat.source,
                "id": chat.id,
                "user_id": chat.user_id,
                "title": chat.title,
                "updated_at": chat.updated_at,
                "message_count": message_count,
                "messages": last,
            })
        })
        .collect();

    Ok(JsonResponse::build().set_list(list).ok("OK"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_last_messages_and_truncates() {
        let long = "x".repeat(MAX_CONTENT_CHARS + 10);
        let msgs: Vec<Value> = (0..8)
            .map(|i| json!({"role": "user", "content": if i == 7 { long.clone() } else { i.to_string() }}))
            .collect();
        let (count, last) = last_messages(&Value::Array(msgs));
        assert_eq!(count, 8);
        assert_eq!(last.len(), MESSAGES_PER_CHAT);
        assert!(last[4]["content"].as_str().unwrap().ends_with('…'));
    }
}
