use axum::{
    extract::Json,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use axum::http::{header, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;

use crate::bridge::call_electron;

const VERSION: &str = "4.1.0";

// --- State Struct ---
#[derive(Debug, Serialize, Clone)]
pub struct HistoryItem {
    pub timestamp: String,
    pub model: String,
    pub query: String,
    pub response: String,
}

#[derive(Debug, Default)]
pub struct AppStats {
    pub total_requests: u64,
    pub total_errors: u64,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub providers: HashMap<String, ProviderStats>,
    pub history: Vec<HistoryItem>,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct ProviderStats {
    pub calls: u64,
    pub errors: u64,
    pub avg_time_ms: u64,
    pub min_time_ms: u64,
    pub max_time_ms: u64,
    pub last_call: Option<String>,
}

pub type SharedStats = Arc<Mutex<AppStats>>;

// --- Models ---
#[derive(Debug, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: Option<Value>, // Can be String or Array of Strings
    pub messages: Option<Vec<Message>>,
    pub message: Option<String>,
    pub query: Option<String>,
    pub prompt: Option<String>,
    pub content: Option<String>,
    pub text: Option<String>,
    pub question: Option<String>,
    pub file: Option<String>,

    // Synapse custom fields
    #[serde(rename = "function")]
    pub function_field: Option<String>,
    pub action: Option<String>,
    pub to: Option<String>,
    pub from: Option<String>,
    pub language: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
    pub topic: Option<String>,
    #[allow(dead_code)]
    pub url: Option<String>,
    pub sides: Option<usize>,
}

// --- Helpers ---
fn extract_message(body: &ChatCompletionRequest) -> Option<String> {
    if let Some(ref messages) = body.messages {
        if let Some(msg) = messages.iter().rfind(|m| m.role == "user") {
            return Some(msg.content.clone());
        }
    }
    body.message
        .clone()
        .or_else(|| body.query.clone())
        .or_else(|| body.prompt.clone())
        .or_else(|| body.content.clone())
        .or_else(|| body.text.clone())
        .or_else(|| body.question.clone())
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

async fn resolve_models(model_input: Option<Value>) -> Result<Vec<String>, String> {
    let enabled = get_enabled_providers().await;
    
    let resolved = match model_input {
        None => vec!["auto".to_string()],
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(arr)) => {
            let mut list = Vec::new();
            for item in arr {
                if let Some(s) = item.as_str() {
                    list.push(s.to_string());
                }
            }
            list
        }
        _ => return Err("Invalid model format".to_string()),
    };

    let mut final_providers = Vec::new();
    for m in resolved {
        let key = m.to_lowercase();
        let provider = match key.as_str() {
            "chatgpt" | "gpt" | "gpt-4" | "gpt-4o" | "gpt-4.5" | "openai" => "chatgpt",
            "claude" | "claude-3" | "claude-3.5" | "claude-4" | "anthropic" | "sonnet" | "opus" | "haiku" => "claude",
            "gemini" | "gemini-pro" | "gemini-2" | "gemini-2.5" | "google" | "bard" => "gemini",
            "perplexity" | "pplx" | "sonar" => "perplexity",
            "deepseek" | "deepseek-coder" => "deepseek",
            "kimi" | "moonshot" => "kimi",
            "all" => "all",
            "auto" => "auto",
            _ => &key,
        };

        if provider == "all" {
            for p in &enabled {
                if !final_providers.contains(p) {
                    final_providers.push(p.clone());
                }
            }
        } else if provider == "auto" {
            let best = ["claude", "chatgpt", "gemini", "perplexity", "deepseek", "kimi"]
                .iter()
                .find(|&&p| enabled.contains(&p.to_string()))
                .map(|&p| p.to_string());
            if let Some(b) = best {
                if !final_providers.contains(&b) {
                    final_providers.push(b);
                }
            }
        } else if enabled.contains(&provider.to_string()) {
            if !final_providers.contains(&provider.to_string()) {
                final_providers.push(provider.to_string());
            }
        } else if provider.starts_with("ollama") || crate::ollama::get_available_models().await.contains(&provider.to_string()) {
            if !final_providers.contains(&provider.to_string()) {
                final_providers.push(provider.to_string());
            }
        }
    }

    if final_providers.is_empty() {
        return Err(format!(
            "No active providers matched. Enabled: {:?}",
            enabled
        ));
    }

    Ok(final_providers)
}

// --- Stats Tracking Helper ---
fn record_call(stats: &SharedStats, provider: &str, time_ms: u64, is_error: bool) {
    let mut s = stats.lock().unwrap();
    s.total_requests += 1;
    if is_error {
        s.total_errors += 1;
    }

    let p = s.providers.entry(provider.to_string()).or_insert_with(|| ProviderStats {
        calls: 0,
        errors: 0,
        avg_time_ms: 0,
        min_time_ms: u64::MAX,
        max_time_ms: 0,
        last_call: None,
    });

    p.calls += 1;
    if is_error {
        p.errors += 1;
        return;
    }

    p.last_call = Some(chrono::Utc::now().to_rfc3339());
    if time_ms < p.min_time_ms {
        p.min_time_ms = time_ms;
    }
    if time_ms > p.max_time_ms {
        p.max_time_ms = time_ms;
    }

    let successful_calls = p.calls - p.errors;
    if successful_calls > 0 {
        p.avg_time_ms = ((p.avg_time_ms * (successful_calls - 1)) + time_ms) / successful_calls;
    }
}

// --- API Implementation ---

async fn query_provider(
    stats: SharedStats,
    provider: &str,
    message: &str,
    file_path: Option<&str>,
) -> Result<Value, String> {
    let start = std::time::Instant::now();
    
    let is_ollama = provider.starts_with("ollama") || crate::ollama::get_available_models().await.contains(&provider.to_string());
    
    if is_ollama {
        match crate::ollama::query_ollama(Some(provider), message).await {
            Ok((text, resolved_model)) => {
                let elapsed = start.elapsed().as_millis() as u64;
                record_call(&stats, &format!("ollama/{}", resolved_model), elapsed, false);
                
                // Push to history
                {
                    if let Ok(mut s) = stats.lock() {
                        s.history.push(HistoryItem {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            model: format!("ollama/{}", resolved_model),
                            query: message.to_string(),
                            response: text.clone(),
                        });
                    }
                }

                return Ok(json!({
                    "text": text,
                    "model": format!("ollama/{}", resolved_model),
                    "responseTimeMs": elapsed
                }));
            }
            Err(e) => {
                record_call(&stats, provider, 0, true);
                return Err(e);
            }
        }
    }
    
    let res = if let Some(path) = file_path {
        call_electron(
            "sendMessageWithFile",
            Some(provider),
            json!({ "message": message, "filePath": path }),
        )
        .await
    } else {
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
                        Ok(json!({ "response": response_str }))
                    } else {
                        call_electron("getResponseWithTyping", Some(provider), json!({}))
                            .await
                    }
                } else {
                    call_electron("getResponseWithTyping", Some(provider), json!({}))
                        .await
                }
            }
            Err(e) => Err(e),
        }
    };

    let elapsed = start.elapsed().as_millis() as u64;

    match res {
        Ok(val) => {
            record_call(&stats, provider, elapsed, false);
            let text = val.get("response").and_then(|r| r.as_str()).unwrap_or("");
            
            // Push to history
            {
                if let Ok(mut s) = stats.lock() {
                    s.history.push(HistoryItem {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        model: provider.to_string(),
                        query: message.to_string(),
                        response: text.to_string(),
                    });
                }
            }

            Ok(json!({
                "text": text,
                "model": provider,
                "responseTimeMs": elapsed
            }))
        }
        Err(e) => {
            // Check if we can fallback to Ollama
            if crate::ollama::is_ollama_online().await {
                println!("[Ollama Fallback] Provider {} failed ({}), falling back to local model...", provider, e);
                let fallback_start = std::time::Instant::now();
                match crate::ollama::query_ollama(None, message).await {
                    Ok((text, resolved_model)) => {
                        let fallback_elapsed = fallback_start.elapsed().as_millis() as u64;
                        record_call(&stats, &format!("ollama/{}", resolved_model), fallback_elapsed, false);
                        
                        let full_text = format!("*[Fallback to local {} due to: {}]*\n\n{}", resolved_model, e, text);
                        
                        // Push to history
                        {
                            if let Ok(mut s) = stats.lock() {
                                s.history.push(HistoryItem {
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    model: format!("ollama/{}", resolved_model),
                                    query: message.to_string(),
                                    response: full_text.clone(),
                                });
                            }
                        }

                        return Ok(json!({
                            "text": full_text,
                            "model": format!("ollama/{}", resolved_model),
                            "responseTimeMs": fallback_elapsed
                        }));
                    }
                    Err(ollama_err) => {
                        println!("[Ollama Fallback] Fallback also failed: {}", ollama_err);
                    }
                }
            }
            
            record_call(&stats, provider, 0, true);
            Err(e)
        }
    }
}

fn format_chat_response(text: &str, provider: &str, response_time_ms: u64) -> Value {
    json!({
        "id": format!("synapse-{}", chrono::Utc::now().timestamp_millis()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": provider,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 },
        "synapse": { "provider": provider, "responseTimeMs": response_time_ms }
    })
}

// --- Handlers ---

async fn handle_completions(
    axum::extract::State((stats, _)): axum::extract::State<(SharedStats, SocketAddr)>,
    Json(body): Json<ChatCompletionRequest>,
) -> impl IntoResponse {
    let fn_field = body.function_field.clone().unwrap_or_default().to_lowercase();
    let model_input = body.model.clone();

    let resolved = match resolve_models(model_input).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": { "message": e, "type": "model_not_found", "code": 404 } })),
            )
                .into_response();
        }
    };

    let message = extract_message(&body).unwrap_or_default();

    // ── function: "search" ──
    if fn_field == "search" {
        if message.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "message or query required" }))).into_response();
        }
        let provider = resolved.first().map(|s| s.as_str()).unwrap_or("perplexity");
        match query_provider(stats, provider, &message, None).await {
            Ok(res) => return Json(format_chat_response(
                res.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                provider,
                res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0),
            )).into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
        }
    }

    // ── function: "translate" ──
    if fn_field == "translate" {
        let to = match body.to {
            Some(t) => t,
            None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "\"to\" field required" }))).into_response(),
        };
        let from_str = body.from.map(|f| format!(" from {}", f)).unwrap_or_default();
        let prompt = format!("Translate the following{} to {}. Only output the translation:\n\n{}", from_str, to, message);
        let provider = resolved.first().map(|s| s.as_str()).unwrap_or("claude");
        match query_provider(stats, provider, &prompt, None).await {
            Ok(res) => return Json(format_chat_response(
                res.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                provider,
                res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0),
            )).into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
        }
    }

    // ── function: "brainstorm" ──
    if fn_field == "brainstorm" {
        let prompt = format!("Brainstorm creative ideas for: {}\n\nProvide diverse, practical suggestions.", message);
        let provider = resolved.first().map(|s| s.as_str()).unwrap_or("claude");
        match query_provider(stats, provider, &prompt, None).await {
            Ok(res) => return Json(format_chat_response(
                res.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                provider,
                res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0),
            )).into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
        }
    }

    // ── function: "code" ──
    if fn_field == "code" {
        let action = body.action.unwrap_or_else(|| "generate".to_string());
        let lang = body.language.unwrap_or_else(|| "JavaScript".to_string());
        let prompt = match action.as_str() {
            "generate" => format!("Generate {} code:\n{}\n\nProvide clean, production-ready code.", lang, message),
            "review" => format!("Review this {} code for bugs, performance, security:\n```{}\n{}\n```", lang, lang, body.code.unwrap_or_default()),
            "debug" => {
                let mut p = "Debug:\n".to_string();
                if let Some(c) = body.code {
                    p += &format!("```{}\n{}\n```\n", lang, c);
                }
                if let Some(err) = body.error {
                    p += &format!("Error: {}\n", err);
                }
                p += "Identify the bug, explain, and fix.";
                p
            }
            "explain" => format!("Explain this {} code:\n```{}\n{}\n```", lang, lang, body.code.unwrap_or_default()),
            _ => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Unknown action" }))).into_response(),
        };

        let provider = resolved.first().map(|s| s.as_str()).unwrap_or("claude");
        match query_provider(stats, provider, &prompt, None).await {
            Ok(res) => return Json(format_chat_response(
                res.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                provider,
                res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0),
            )).into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
        }
    }

    // ── function: "security_audit" ──
    if fn_field == "security_audit" {
        let code = body.code.unwrap_or(message.clone());
        let lang = body.language.unwrap_or_else(|| "code".to_string());
        let prompt = format!(
            "You are a senior security engineer. Perform a thorough security audit of this code ({lang}).\n\nCODE:\n{code}\n\nCheck for: injection vulnerabilities, auth flaws, data exposure, input validation issues, cryptographic issues, config problems, dependency risks.\n\nFor each issue: Severity (CRITICAL/HIGH/MEDIUM/LOW), Location, Description, Fix. End with a security score (0-100)."
        );
        let provider = resolved.first().map(|s| s.as_str()).unwrap_or("claude");
        match query_provider(stats, provider, &prompt, None).await {
            Ok(res) => return Json(format_chat_response(
                res.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                provider,
                res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0),
            )).into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
        }
    }

    // ── function: "debate" ──
    if fn_field == "debate" {
        let topic = body.topic.unwrap_or(message.clone());
        let sides = body.sides.unwrap_or(2);
        
        if resolved.len() < 2 {
            let prompt = format!("Debate this topic from {sides} different perspectives with strong arguments and evidence:\n\nTopic: {topic}\n\nFormat: ## Perspective [N]: [Position]\n- Arguments\n- Evidence\n\nEnd with balanced conclusion.");
            let provider = resolved.first().map(|s| s.as_str()).unwrap_or("claude");
            match query_provider(stats, provider, &prompt, None).await {
                Ok(res) => return Json(format_chat_response(
                    res.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                    provider,
                    res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0),
                )).into_response(),
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
            }
        }

        // Multi-provider debate
        let stances = vec![
            "FOR / supportive",
            "AGAINST / critical",
            "NEUTRAL / analytical",
            "ALTERNATIVE / unconventional",
        ];
        
        let mut perspectives = json!({});
        let mut timings = json!({});
        
        for (i, provider) in resolved.iter().take(sides).enumerate() {
            let stance = stances.get(i).unwrap_or(&"Alternative");
            let prompt = format!("You are debating this topic. Your position: {stance}.\n\nTopic: {topic}\n\nPresent your strongest arguments. Be persuasive. Do NOT present the other side.");
            
            match query_provider(Arc::clone(&stats), provider, &prompt, None).await {
                Ok(res) => {
                    perspectives[provider] = json!({
                        "stance": stance,
                        "response": res.get("text").and_then(|t| t.as_str()).unwrap_or("")
                    });
                    timings[provider] = res.get("responseTimeMs").unwrap_or(&json!(0)).clone();
                }
                Err(e) => {
                    perspectives[provider] = json!({
                        "stance": stance,
                        "error": e
                    });
                }
            }
        }

        return Json(json!({
            "id": format!("synapse-{}", chrono::Utc::now().timestamp_millis()),
            "object": "chat.completion",
            "model": "debate",
            "topic": topic,
            "perspectives": perspectives,
            "timings": timings,
            "synapse": { "function": "debate", "providers": resolved.iter().take(sides).collect::<Vec<_>>() }
        })).into_response();
    }

    // ── Normal Chat (Default) ──
    if resolved.len() == 1 {
        let provider = &resolved[0];
        match query_provider(stats, provider, &message, body.file.as_deref()).await {
            Ok(res) => {
                let text = res.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let time = res.get("responseTimeMs").and_then(|t| t.as_u64()).unwrap_or(0);
                Json(format_chat_response(text, provider, time)).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
        }
    } else {
        // Multi-model completions
        let mut results = json!({});
        let mut timings = json!({});
        
        for provider in &resolved {
            match query_provider(Arc::clone(&stats), provider, &message, body.file.as_deref()).await {
                Ok(res) => {
                    results[provider] = res.get("text").unwrap_or(&json!("")).clone();
                    timings[provider] = res.get("responseTimeMs").unwrap_or(&json!(0)).clone();
                }
                Err(e) => {
                    results[provider] = json!(null);
                    timings[provider] = json!({ "error": e });
                }
            }
        }
        
        Json(json!({
            "id": format!("synapse-{}", chrono::Utc::now().timestamp_millis()),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": "all",
            "choices": resolved.iter().enumerate().map(|(idx, p)| {
                json!({
                    "index": idx,
                    "message": { "role": "assistant", "content": results[p] },
                    "finish_reason": "stop",
                    "model": p,
                    "responseTimeMs": timings[p]
                })
            }).collect::<Vec<_>>(),
            "synapse": { "providers": resolved, "timings": timings }
        })).into_response()
    }
}

async fn handle_list_models() -> impl IntoResponse {
    let enabled = get_enabled_providers().await;
    let all_providers = vec!["chatgpt", "claude", "gemini", "perplexity", "deepseek", "kimi"];
    
    let mut data = Vec::new();
    
    for p in &enabled {
        data.push(json!({
            "id": p,
            "object": "model",
            "created": chrono::Utc::now().timestamp(),
            "owned_by": "synapse",
            "status": "enabled"
        }));
    }
    
    for p in all_providers {
        if !enabled.contains(&p.to_string()) {
            data.push(json!({
                "id": p,
                "object": "model",
                "owned_by": "synapse",
                "status": "disabled"
            }));
        }
    }
    
    // Append any available local Ollama models if online
    if crate::ollama::is_ollama_online().await {
        for m in crate::ollama::get_available_models().await {
            data.push(json!({
                "id": format!("ollama/{}", m),
                "object": "model",
                "owned_by": "ollama",
                "status": "enabled"
            }));
        }
    }
    
    Json(json!({ "object": "list", "data": data }))
}

async fn handle_new_conversations() -> impl IntoResponse {
    let enabled = get_enabled_providers().await;
    for p in enabled {
        let _ = call_electron("newConversation", Some(&p), json!({})).await;
    }
    Json(json!({ "success": true, "message": "All active conversations reset" }))
}

async fn handle_stats(axum::extract::State((stats, _)): axum::extract::State<(SharedStats, SocketAddr)>) -> impl IntoResponse {
    let s = stats.lock().unwrap();
    let uptime_sec = match s.start_time {
        Some(st) => chrono::Utc::now().signed_duration_since(st).num_seconds(),
        None => 0,
    };
    
    Json(json!({
        "uptime": format!("{}s", uptime_sec),
        "totalRequests": s.total_requests,
        "totalErrors": s.total_errors,
        "providers": s.providers
    }))
}

async fn handle_history(axum::extract::State((stats, _)): axum::extract::State<(SharedStats, SocketAddr)>) -> impl IntoResponse {
    let s = stats.lock().unwrap();
    Json(json!({
        "history": s.history
    }))
}

async fn handle_status(axum::extract::State((stats, _)): axum::extract::State<(SharedStats, SocketAddr)>) -> impl IntoResponse {
    let enabled = get_enabled_providers().await;
    let s = stats.lock().unwrap();
    let uptime_sec = match s.start_time {
        Some(st) => chrono::Utc::now().signed_duration_since(st).num_seconds(),
        None => 0,
    };

    Json(json!({
        "status": "online",
        "version": VERSION,
        "enabledProviders": enabled,
        "stats": {
            "uptime": format!("{}s", uptime_sec),
            "totalRequests": s.total_requests,
            "totalErrors": s.total_errors
        }
    }))
}

// --- Document Static Pages ---

async fn get_docs_page() -> Html<String> {
    Html(r#"<!DOCTYPE html>
<html>
<head>
    <title>Synapse API</title>
</head>
<body style="font-family: sans-serif; background: #0c0c16; color: #fff; padding: 40px;">
    <h1>⚡ Synapse Local REST API running (Rust sidecar)</h1>
    <p>API Endpoint: <code>POST http://localhost:3210/v1/chat/completions</code></p>
    <p>Check the project README for usage details.</p>
</body>
</html>"#.to_string())
}

// --- RAG Endpoints ---

#[derive(Debug, serde::Deserialize)]
pub struct RagSearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 5 }

async fn handle_rag_search(
    Json(body): Json<RagSearchRequest>,
) -> impl IntoResponse {
    let results = crate::rag::search_index(&body.query, body.limit);
    Json(json!({
        "query": body.query,
        "results": results,
        "count": results.len()
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct RagIndexRequest {
    pub directory: Option<String>,
}

async fn handle_rag_index(
    Json(body): Json<RagIndexRequest>,
) -> impl IntoResponse {
    let dir = body.directory.unwrap_or_else(|| ".".to_string());
    crate::rag::init_index(&dir);
    Json(json!({
        "success": true,
        "indexed_directory": dir
    }))
}

// --- Router Setup ---
pub fn setup_router(stats: SharedStats, port: SocketAddr) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(handle_completions))
        .route("/v1/models", get(handle_list_models))
        .route("/v1/conversations/new", post(handle_new_conversations))
        .route("/v1/stats", get(handle_stats))
        .route("/v1/history", get(handle_history))
        .route("/v1/rag/search", post(handle_rag_search))
        .route("/v1/rag/index", post(handle_rag_index))
        .route("/api/status", get(handle_status))
        .route("/", get(get_docs_page))
        .route("/docs", get(get_docs_page))
        .route("/ws", get(crate::ws_server::ws_handler))
        .route("/websocket", get(crate::ws_server::ws_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(vec![header::CONTENT_TYPE, header::AUTHORIZATION]),
        )
        .with_state((stats, port))
}
