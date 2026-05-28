use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use crate::bridge::call_electron;

const IPC_PORT: u16 = 19222;

#[derive(Debug, Deserialize)]
struct IPCRequest {
    #[serde(rename = "requestId")]
    request_id: Value,
    action: String,
    provider: Option<String>,
    #[serde(default)]
    data: Value,
}

pub async fn start_ipc_server() {
    let addr = format!("127.0.0.1:{}", IPC_PORT);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[IPC Server] Failed to bind to {}: {}", addr, e);
            return;
        }
    };

    println!("[IPC Server] Listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok((socket, client_addr)) => {
                tokio::spawn(async move {
                    handle_client(socket, client_addr).await;
                });
            }
            Err(e) => {
                eprintln!("[IPC Server] Connection accept failed: {}", e);
            }
        }
    }
}

async fn handle_client(mut socket: tokio::net::TcpStream, addr: std::net::SocketAddr) {
    println!("[IPC Server] Client connected from {}", addr);
    let (reader, mut writer) = socket.split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match buf_reader.read_line(&mut line).await {
            Ok(0) => {
                // Connection closed
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let req: Result<IPCRequest, _> = serde_json::from_str(trimmed);
                match req {
                    Ok(request) => {
                        let req_id = request.request_id.clone();
                        
                        let response_val = if request.action == "workspaceRagSearch" {
                            let query = request.data.get("query").and_then(|q| q.as_str()).unwrap_or("");
                            let limit = request.data.get("limit").and_then(|l| l.as_u64()).unwrap_or(5) as usize;
                            let results = crate::rag::search_index(query, limit);
                            json!({
                                "requestId": req_id,
                                "success": true,
                                "result": results
                            })
                        } else if request.action == "workspaceRagIndex" {
                            let dir = request.data.get("directory").and_then(|d| d.as_str()).unwrap_or(".");
                            crate::rag::init_index(dir);
                            json!({
                                "requestId": req_id,
                                "success": true,
                                "result": { "message": "Indexing complete" }
                            })
                        } else {
                            let res = call_electron(
                                &request.action,
                                request.provider.as_deref(),
                                request.data.clone(),
                            )
                            .await;

                            match res {
                                Ok(val) => {
                                    json!({
                                        "requestId": req_id,
                                        "success": true,
                                        "result": val
                                    })
                                }
                                Err(e) => {
                                    json!({
                                        "requestId": req_id,
                                        "success": false,
                                        "error": e
                                    })
                                }
                            }
                        };

                        let mut resp_line = response_val.to_string();
                        resp_line.push('\n');

                        if writer.write_all(resp_line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let err_resp = json!({
                            "success": false,
                            "error": format!("Invalid JSON request: {}", e)
                        });
                        let mut resp_line = err_resp.to_string();
                        resp_line.push('\n');
                        if writer.write_all(resp_line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[IPC Server] Error reading from {}: {}", addr, e);
                break;
            }
        }
    }

    println!("[IPC Server] Client disconnected: {}", addr);
}
