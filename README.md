<div align="center">

<img src="assets/synapse-icon.png" alt="Synapse" width="80"/>

# Synapse

**A Unified Desktop AI Hub & Local Gateway for Coding Tools**

Bring your existing browser-session models (ChatGPT, Claude, Gemini, Perplexity, DeepSeek, Kimi) and local LLMs (Ollama) straight into your IDE and terminal — without needing separate paid API keys.

<br>

[![Version](https://img.shields.io/badge/version-4.1.0-blue.svg)](#)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey.svg)](#)

<br>

[Overview](#-overview) · [Core Features](#-core-features) · [Installation](#%EF%B8%8F-installation) · [Connecting to Editors](#-connecting-to-editors) · [CLI Tool](#-cli-tool) · [REST API](#-rest-api) · [Workspace RAG](#-workspace-rag) · [How it Works](#%EF%B8%8F-how-it-works)

</div>

---

## 🌟 Overview

**Synapse** acts as a secure local API gateway and desktop client that routes queries from your development tools to Web AI providers at the browser session level. By utilizing your active browser logins, Synapse lets you leverage premium frontier models directly in your workflow. 

It pairs an **Electron client** (running a modern Svelte+TypeScript dashboard) with a high-performance **Rust backend** (running an Axum REST/WebSocket server and workspace indexer).

---

## ⚡ Core Features

* 🤖 **6 Browser-Session Engines:** Fully supports ChatGPT, Claude, Gemini, Perplexity, **DeepSeek**, and **Kimi** (Moonshot).
* 🎛️ **Local Model Fallback:** Integrates with **Ollama**. Routes queries locally if you are offline or if browser sessions fail.
* 🔎 **Global Spotlight Bar:** A sleek, glassmorphic floating search bar (`⌥ Option + Space` on macOS / `Alt + Space` on Windows/Linux) for quick AI queries.
* 📁 **Workspace RAG Indexing:** Automatically indexes local directories and retrieves relevant code context semantically.
* 🖥️ **Command Line Interface (CLI):** Query models, pipe terminal logs, and upload files/images directly from your terminal.
* 🔌 **Standard MCP Server:** Connects directly with Cursor, VS Code, Windsurf, or Claude Desktop.
* 📊 **Analytics Telemetry:** Real-time dashboard showing provider response speeds, success rates, request logs, and historical exports.

---

## 🛠️ Installation

### 1. Run from Source
Clone the repository and install the dependencies for both the Electron client and Svelte UI:

```bash
# Clone the repository
git clone https://github.com/Velodev-io/Synapse.git
cd Synapse

# Install root dependencies
npm install

# Install Svelte UI dependencies
cd frontend && npm install && cd ..

# Launch the application
npm start
```
> *Note: On first startup, it will compile the Rust backend (`synapse-backend`). This may take 15-30 seconds.*

### 2. Configure CLI Link
To make the CLI tool available globally under the name `synapse`:
* **macOS / Linux:** `sudo npm link`
* **Windows:** Settings ➔ **Install CLI to PATH**

---

## 🔌 Connecting to Editors

Synapse operates as an MCP (Model Context Protocol) server. Copy the configuration from the **Settings** tab in the application and paste it into your editor's MCP settings:

```json
{
  "mcpServers": {
    "synapse": {
      "command": "node",
      "args": ["/Users/binova/Documents/Projects/Suru/Test/Proxima/src/mcp-server-v3.js"]
    }
  }
}
```

**Compatible Editors:** Cursor, VS Code (via MCP extensions), Windsurf, Claude Desktop, and more.

---

## 🖥️ CLI Tool

The CLI allows you to execute quick queries and process terminal inputs directly:

```bash
# Ask questions to the default model
synapse ask "Explain how Axum routing works"

# Ask specific models (chatgpt, claude, gemini, perplexity, deepseek, kimi, auto)
synapse ask claude "Review this logic"
synapse ask deepseek "Optimize this function"

# Upload images/PDFs from the command line
synapse ask chatgpt --image ./screenshots/error.png "Explain this crash"
synapse ask claude --pdf ~/documents/api-docs.pdf "Summarize the endpoints"

# Pipe build logs or git diffs directly
npm run build 2>&1 | synapse fix
git diff | synapse code review
```

---

## 📡 REST API

Synapse hosts an OpenAI-compatible REST server at `http://127.0.0.1:3210` supporting:
* `GET /api/status` — Endpoint health and list of enabled provider engines.
* `POST /v1/chat/completions` — Standard Chat Completions (supporting streaming and cache).
* `GET /v1/models` — List of available provider models.
* `GET /v1/stats` / `GET /v1/history` — Usage statistics and request telemetry.

---

## 🧠 Workspace RAG

The Rust backend includes a built-in semantic search engine to search and index local files:

* **Index a Workspace:**
  ```bash
  curl -X POST http://127.0.0.1:3210/v1/rag/index \
    -H "Content-Type: application/json" \
    -d '{"directory": "/path/to/project"}'
  ```
* **Semantic Search:**
  ```bash
  curl -X POST http://127.0.0.1:3210/v1/rag/search \
    -H "Content-Type: application/json" \
    -d '{"query": "Ollama fallback logic", "limit": 3}'
  ```

---

## ⚙️ How it Works

Synapse coordinates between three primary components:

```
┌────────────────────────────────┐
│   IDE / Terminal (CLI / MCP)   │
└───────────────┬────────────────┘
                │ HTTP / WebSocket / TCP IPC
                ▼
┌────────────────────────────────┐
│  Rust Axum Server (Port 3210)  │ ─── [Ollama / Local Fallback]
└───────────────┬────────────────┘
                │ Loopback Callback
                ▼
┌────────────────────────────────┐
│ Electron Client (BrowserViews) │ ─── [Stealth Preload Injection]
└────────────────────────────────┘
```

1. **Local Server (Rust):** Listens for incoming API/MCP requests and handles RAG search indexing.
2. **Stealth Preload:** When web sessions are created, a custom preload script (`provider-preload.cjs`) runs in the browser context to set platform-consistent headers, spoof `navigator.webdriver`, and bypass Google OAuth blocks.
3. **Session Engines:** Injected engine scripts execute requests in the web pages' contexts and stream results back in real time.
