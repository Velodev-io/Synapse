use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

pub async fn is_ollama_online() -> bool {
    let client = reqwest::Client::new();
    let res = client.get("http://127.0.0.1:11434/api/tags")
        .timeout(Duration::from_millis(800))
        .send()
        .await;
    res.is_ok()
}

pub async fn get_available_models() -> Vec<String> {
    let client = reqwest::Client::new();
    let res = client.get("http://127.0.0.1:11434/api/tags")
        .timeout(Duration::from_secs(2))
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(data) = response.json::<OllamaTagsResponse>().await {
                    return data.models.into_iter().map(|m| m.name).collect();
                }
            }
            vec![]
        }
        Err(_) => vec![],
    }
}

#[allow(dead_code)]
pub async fn get_first_model() -> Option<String> {
    let models = get_available_models().await;
    models.first().cloned()
}

pub async fn query_ollama(model_opt: Option<&str>, prompt: &str) -> Result<(String, String), String> {
    let client = reqwest::Client::new();
    
    // Resolve model name
    let requested_model = model_opt.unwrap_or("");
    let clean_model = if requested_model.starts_with("ollama/") {
        requested_model.trim_start_matches("ollama/")
    } else {
        requested_model
    };

    let available = get_available_models().await;
    let model = if !clean_model.is_empty() && available.contains(&clean_model.to_string()) {
        clean_model.to_string()
    } else if let Some(first) = available.first() {
        first.clone()
    } else {
        return Err("No local models found in Ollama. Make sure Ollama is running and you have pulled a model (e.g. `ollama run llama3`).".to_string());
    };

    let payload = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ],
        "stream": false
    });

    let res = client.post("http://127.0.0.1:11434/api/chat")
        .timeout(Duration::from_secs(120))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Ollama error status {}: {}", status, err_text));
    }

    let chat_res = res.json::<OllamaChatResponse>()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    Ok((chat_res.message.content, model))
}
