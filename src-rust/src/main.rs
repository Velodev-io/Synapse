use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

mod bridge;
mod ipc_server;
mod rest_server;
mod ws_server;
mod ollama;
mod rag;

#[tokio::main]
async fn main() {
    println!("=== Synapse Backend Server (Rust) ===");

    // Shared statistics state
    let stats = Arc::new(Mutex::new(rest_server::AppStats {
        total_requests: 0,
        total_errors: 0,
        start_time: Some(chrono::Utc::now()),
        providers: std::collections::HashMap::new(),
        history: Vec::new(),
    }));

    // Start TCP IPC Server for MCP client on port 19222
    tokio::spawn(async {
        ipc_server::start_ipc_server().await;
    });

    // Asynchronously index current workspace directory on startup
    tokio::spawn(async {
        if let Ok(cwd) = std::env::current_dir() {
            let cwd_str = cwd.to_string_lossy().to_string();
            rag::init_index(&cwd_str);
        }
    });

    // Start REST & WS Server on port 3210
    let rest_port = std::env::var("SYNAPSE_REST_PORT")
        .unwrap_or_else(|_| "3210".to_string())
        .parse::<u16>()
        .unwrap_or(3210);

    let addr = SocketAddr::from(([127, 0, 0, 1], rest_port));
    
    // Add both the REST router and the WebSocket upgrade handler
    let app = rest_server::setup_router(Arc::clone(&stats), addr);

    println!("[REST & WS] Server listening on http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[REST & WS] Failed to bind to http://{}: {}", addr, e);
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[REST & WS] Server error: {}", e);
    }
}
