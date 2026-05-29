use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::bridge::call_electron;
use crate::rest_server::SharedStats;

#[derive(Debug, Deserialize)]
struct WSMessage {
    action: Option<String>,
    id: Option<String>,
    model: Option<String>,
    message: Option<String>,
    query: Option<String>,
    topic: Option<String>,
    code: Option<String>,
    text: Option<String>,
    to: Option<String>,
    from: Option<String>,
    language: Option<String>,
    description: Option<String>,
    subaction: Option<String>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct WSResponse {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "responseTimeMs")]
    response_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    results: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    providers: Option<Vec<String>>,
    timestamp: String,
}

// Global active connections counter
static ACTIVE_CONNECTIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static TOTAL_CONNECTIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State((stats, _)): State<(SharedStats, SocketAddr)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, stats))
}

async fn handle_socket(socket: WebSocket, stats: SharedStats) {
    let (mut sender, mut receiver) = socket.split();
    let client_id = format!("ws_{}_{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4().to_string().get(..6).unwrap_or(""));

    ACTIVE_CONNECTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    TOTAL_CONNECTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    println!("[WS] Client connected: {} ({} active)", client_id, ACTIVE_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst));

    // Welcome message
    let welcome = json!({
        "type": "connected",
        "clientId": client_id,
        "version": "4.1.0",
        "message": "Connected to Synapse WebSocket (Rust)",
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    
    if sender.send(Message::Text(welcome.to_string())).await.is_err() {
        return;
    }

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            let parsed_msg: Result<WSMessage, _> = serde_json::from_str(&text);
            match parsed_msg {
                Ok(ws_msg) => {
                    let response_sender = Arc::new(tokio::sync::Mutex::new(sender));
                    let stats_clone = Arc::clone(&stats);
                    let client_id_clone = client_id.clone();
                    
                    let sender_for_task = Arc::clone(&response_sender);
                    tokio::spawn(async move {
                        process_message(ws_msg, sender_for_task, stats_clone, client_id_clone).await;
                    });
                    
                    // Retrieve sender back to loop
                    sender = match Arc::try_unwrap(response_sender) {
                        Ok(mutex) => mutex.into_inner(),
                        Err(_) => break, // If there's an active borrow we shouldn't continue
                    };
                }
                Err(_) => {
                    let err_resp = json!({
                        "type": "error",
                        "error": "Invalid JSON",
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    });
                    if sender.send(Message::Text(err_resp.to_string())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    ACTIVE_CONNECTIONS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    println!("[WS] Client disconnected: {} ({} active)", client_id, ACTIVE_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst));
}

async fn get_enabled_providers() -> Vec<String> {
    match call_electron("getSettings", None, json!({})).await {
        Ok(settings) => {
            let mut enabled = Vec::new();
            if let Some(providers) = settings.get("settings").and_then(|s| s.get("providers")).and_then(|p| p.as_object()) {
                for (name, conf) in providers {
                    if conf.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false) {
                        enabled.push(name.clone());
                    }
                }
            }
            if enabled.is_empty() {
                enabled = vec!["perplexity".to_string(), "chatgpt".to_string(), "gemini".to_string()];
            }
            enabled
        }
        Err(_) => vec!["perplexity".to_string(), "chatgpt".to_string(), "gemini".to_string()],
    }
}

async fn pick_best_provider(preferred: Option<String>) -> Option<String> {
    let enabled = get_enabled_providers().await;
    if let Some(p) = preferred {
        if p != "auto" {
            let alias = p.to_lowercase();
            if enabled.contains(&alias) {
                return Some(alias);
            }
            return None;
        }
    }
    ["claude", "chatgpt", "gemini", "perplexity", "deepseek", "kimi"]
        .iter()
        .find(|&&p| enabled.contains(&p.to_string()))
        .map(|&p| p.to_string())
}

async fn query_provider_text(provider: &str, message: &str) -> Result<String, String> {
    // Normal sendMessage -> getResponseWithTyping
    let send_res = call_electron(
        "sendMessage",
        Some(provider),
        json!({ "message": message }),
    )
    .await;

    match send_res {
        Ok(val) => {
            if let Some(response_str) = val.get("response").and_then(|r| r.as_str()) {
                if !response_str.is_empty() {
                    return Ok(response_str.to_string());
                }
            }
            let resp = call_electron("getResponseWithTyping", Some(provider), json!({})).await?;
            Ok(resp.get("response").and_then(|r| r.as_str()).unwrap_or("").to_string())
        }
        Err(e) => Err(e),
    }
}

async fn process_message(
    msg: WSMessage,
    sender: Arc<tokio::sync::Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
    stats: SharedStats,
    _client_id: String,
) {
    let req_id = msg.id.clone().unwrap_or_else(|| format!("req_{}", chrono::Utc::now().timestamp_millis()));
    let action = match msg.action.as_deref() {
        Some(a) => a,
        None => {
            let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": "Missing action field" })).await;
            return;
        }
    };

    match action {
        "ask" | "chat" => {
            let prompt = msg.message.or(msg.query).unwrap_or_default();
            if prompt.is_empty() {
                let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": "Missing message" })).await;
                return;
            }
            let provider = match pick_best_provider(msg.model).await {
                Some(p) => p,
                None => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": "No providers available" })).await;
                    return;
                }
            };
            
            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "processing",
                "model": provider,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            match query_provider_text(&provider, &prompt).await {
                Ok(content) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let _ = send_json_ws(&sender, json!({
                        "type": "response",
                        "id": req_id,
                        "action": "ask",
                        "model": provider,
                        "content": content,
                        "responseTimeMs": elapsed,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).await;
                }
                Err(e) => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": e })).await;
                }
            }
        }

        "search" => {
            let prompt = msg.query.or(msg.message).unwrap_or_default();
            let provider = pick_best_provider(Some("perplexity".to_string())).await.unwrap_or_else(|| "perplexity".to_string());
            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "searching",
                "model": provider,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            match query_provider_text(&provider, &prompt).await {
                Ok(content) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let _ = send_json_ws(&sender, json!({
                        "type": "response",
                        "id": req_id,
                        "action": "search",
                        "model": provider,
                        "content": content,
                        "responseTimeMs": elapsed,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).await;
                }
                Err(e) => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": e })).await;
                }
            }
        }

        "code" => {
            let desc = msg.description.or(msg.message).unwrap_or_default();
            let subaction = msg.subaction.unwrap_or_else(|| "generate".to_string());
            let lang = msg.language.unwrap_or_else(|| "JavaScript".to_string());
            let provider = pick_best_provider(None).await.unwrap_or_else(|| "claude".to_string());
            
            let prompts = [
                ("generate", format!("Generate {lang} code for: {desc}")),
                ("review", format!("Review this code for bugs, improvements, and best practices:\n\n{desc}")),
                ("explain", format!("Explain this code in detail:\n\n{desc}")),
                ("optimize", format!("Optimize this code for performance:\n\n{desc}")),
                ("debug", format!("Debug this code and find the issue:\n\n{desc}"))
            ];
            
            let prompt = prompts.iter().find(|(k, _)| k == &subaction.as_str()).map(|(_, v)| v.clone()).unwrap_or_else(|| format!("Generate code for: {desc}"));

            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "coding",
                "model": provider,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            match query_provider_text(&provider, &prompt).await {
                Ok(content) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let _ = send_json_ws(&sender, json!({
                        "type": "response",
                        "id": req_id,
                        "action": "code",
                        "subaction": subaction,
                        "model": provider,
                        "content": content,
                        "responseTimeMs": elapsed,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).await;
                }
                Err(e) => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": e })).await;
                }
            }
        }

        "translate" => {
            let input = msg.text.or(msg.message).unwrap_or_default();
            let to = msg.to.unwrap_or_else(|| "English".to_string());
            let from_str = msg.from.map(|f| format!("from {f} ")).unwrap_or_default();
            let prompt = format!("Translate the following {from_str}to {to}:\n\n{input}");
            let provider = pick_best_provider(None).await.unwrap_or_else(|| "claude".to_string());

            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "translating",
                "model": provider,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            match query_provider_text(&provider, &prompt).await {
                Ok(content) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let _ = send_json_ws(&sender, json!({
                        "type": "response",
                        "id": req_id,
                        "action": "translate",
                        "model": provider,
                        "content": content,
                        "responseTimeMs": elapsed,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).await;
                }
                Err(e) => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": e })).await;
                }
            }
        }

        "brainstorm" => {
            let subject = msg.topic.or(msg.message).unwrap_or_default();
            let provider = pick_best_provider(None).await.unwrap_or_else(|| "claude".to_string());
            let prompt = format!("Brainstorm creative and innovative ideas about: {subject}\n\nProvide at least 5-8 diverse ideas with brief explanations.");

            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "brainstorming",
                "model": provider,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            match query_provider_text(&provider, &prompt).await {
                Ok(content) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let _ = send_json_ws(&sender, json!({
                        "type": "response",
                        "id": req_id,
                        "action": "brainstorm",
                        "model": provider,
                        "content": content,
                        "responseTimeMs": elapsed,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).await;
                }
                Err(e) => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": e })).await;
                }
            }
        }

        "debate" => {
            let subject = msg.topic.or(msg.message).unwrap_or_default();
            let enabled = get_enabled_providers().await;
            if enabled.is_empty() {
                let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": "No providers available for debate" })).await;
                return;
            }

            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "debating",
                "providers": enabled,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            let mut results = json!({});
            
            for provider in &enabled {
                let _ = send_json_ws(&sender, json!({
                    "type": "status",
                    "id": req_id,
                    "status": format!("asking {provider}..."),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })).await;

                let prompt = format!("Give your perspective on this topic. Be direct and opinionated:\n\n{subject}");
                match query_provider_text(provider, &prompt).await {
                    Ok(content) => {
                        results[provider] = json!(content);
                    }
                    Err(e) => {
                        results[provider] = json!(format!("Error: {e}"));
                    }
                }
            }

            let elapsed = start.elapsed().as_millis() as u64;
            let _ = send_json_ws(&sender, json!({
                "type": "response",
                "id": req_id,
                "action": "debate",
                "topic": subject,
                "results": results,
                "providers": enabled,
                "responseTimeMs": elapsed,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;
        }

        "audit" | "security_audit" => {
            let code = msg.code.or(msg.message).unwrap_or_default();
            let provider = pick_best_provider(None).await.unwrap_or_else(|| "claude".to_string());
            let prompt = format!("Perform a security audit on this code. Identify vulnerabilities (SQL injection, XSS, CSRF, etc.), rate severity, and suggest fixes:\n\n{code}");

            let _ = send_json_ws(&sender, json!({
                "type": "status",
                "id": req_id,
                "status": "auditing",
                "model": provider,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;

            let start = Instant::now();
            match query_provider_text(&provider, &prompt).await {
                Ok(content) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let _ = send_json_ws(&sender, json!({
                        "type": "response",
                        "id": req_id,
                        "action": "security_audit",
                        "model": provider,
                        "content": content,
                        "responseTimeMs": elapsed,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).await;
                }
                Err(e) => {
                    let _ = send_json_ws(&sender, json!({ "type": "error", "id": req_id, "error": e })).await;
                }
            }
        }

        "ping" => {
            let _ = send_json_ws(&sender, json!({ "type": "pong", "id": req_id, "timestamp": chrono::Utc::now().to_rfc3339() })).await;
        }

        "stats" => {
            let enabled = get_enabled_providers().await;
            let (total_requests, total_errors, uptime_sec) = {
                let s = stats.lock().unwrap();
                let uptime_sec = match s.start_time {
                    Some(st) => chrono::Utc::now().signed_duration_since(st).num_seconds(),
                    None => 0,
                };
                (s.total_requests, s.total_errors, uptime_sec)
            };

            let _ = send_json_ws(&sender, json!({
                "type": "stats",
                "id": req_id,
                "data": {
                    "totalConnections": TOTAL_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst),
                    "activeConnections": ACTIVE_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst),
                    "totalMessages": total_requests,
                    "totalErrors": total_errors,
                    "uptime": format!("{uptime_sec}s"),
                    "enabledProviders": enabled,
                },
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;
        }

        "new_conversation" | "new" | "reset" => {
            let enabled = get_enabled_providers().await;
            for provider in &enabled {
                let _ = call_electron("newConversation", Some(provider), json!({})).await;
            }
            let _ = send_json_ws(&sender, json!({
                "type": "response",
                "id": req_id,
                "action": "new_conversation",
                "message": "All conversations reset",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).await;
        }

        _ => {
            let _ = send_json_ws(&sender, json!({
                "type": "error",
                "id": req_id,
                "error": format!("Unknown action: {}", action)
            })).await;
        }
    }
}

async fn send_json_ws(
    sender: &Arc<tokio::sync::Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
    val: Value,
) -> Result<(), axum::Error> {
    let mut guard = sender.lock().await;
    guard.send(Message::Text(val.to_string())).await
}
