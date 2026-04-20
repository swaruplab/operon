//! Translation proxy manager for OpenAI-compatible endpoints.
//!
//! Claude Code speaks the Anthropic Messages API (`POST /v1/messages`) exclusively.
//! Ollama, vLLM, LM Studio (pre-0.4.1), and many in-house proxies speak only the
//! OpenAI Chat Completions API (`POST /v1/chat/completions`). To make them work
//! with Claude Code we bundle the `anthropic-proxy-rs` sidecar (MIT, Rust,
//! ~3 MB) that translates between the two formats.
//!
//! Lifecycle:
//!   - Lazily started when a Claude session begins with `ai_provider == "custom"`
//!     and `use_translation_proxy == true`.
//!   - Bound to a random free port on 127.0.0.1.
//!   - Killed on app close (window close event in lib.rs) and on settings change.
//!
//! Once running, `ProxyManager::proxy_base_url()` returns `http://127.0.0.1:<port>`.
//! The `ai_provider_env` helper in `claude.rs` consults this and substitutes the
//! proxy URL for the user's upstream URL when sending env vars to Claude Code.

use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

pub struct ProxyHandle {
    pub child: CommandChild,
    pub port: u16,
    pub upstream_base_url: String,
}

#[derive(Default)]
pub struct ProxyManager {
    pub handle: Mutex<Option<ProxyHandle>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// URL Claude Code should point at when the proxy is running.
    /// Returns None when no proxy is currently bound.
    pub fn proxy_base_url(&self) -> Option<String> {
        let guard = self.handle.lock().ok()?;
        guard
            .as_ref()
            .map(|h| format!("http://127.0.0.1:{}", h.port))
    }

    /// Kill any running proxy.
    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.handle.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = guard.take() {
            let _ = handle.child.kill();
        }
        Ok(())
    }
}

fn pick_free_port() -> Result<u16, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Bind probe failed: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Port query failed: {}", e))?
        .port();
    drop(listener);
    Ok(port)
}

async fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Start (or restart) the translation proxy. Returns the local URL Claude Code should use.
#[tauri::command]
pub async fn start_translation_proxy(
    app: AppHandle,
    state: tauri::State<'_, ProxyManager>,
    upstream_base_url: String,
    upstream_api_key: Option<String>,
) -> Result<String, String> {
    // Kill any existing proxy first.
    state.stop()?;

    let upstream = upstream_base_url.trim().trim_end_matches('/').to_string();
    if upstream.is_empty() {
        return Err("Upstream base URL is required".to_string());
    }

    let port = pick_free_port()?;

    let shell = app.shell();
    let mut cmd = shell
        .sidecar("anthropic-proxy")
        .map_err(|e| format!("Failed to locate anthropic-proxy sidecar: {}", e))?
        .env("UPSTREAM_BASE_URL", &upstream)
        .env("PORT", port.to_string());

    if let Some(key) = upstream_api_key.as_ref() {
        if !key.is_empty() {
            cmd = cmd.env("UPSTREAM_API_KEY", key);
        }
    }

    let (mut rx, child) = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn anthropic-proxy sidecar: {}", e))?;

    // Drain stdout/stderr so the child doesn't block. Emit log lines to the
    // frontend so a status panel can surface them.
    let app_for_logs = app.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                    let line = String::from_utf8_lossy(&bytes).to_string();
                    let _ = app_for_logs.emit("translation-proxy-log", line);
                }
                CommandEvent::Terminated(payload) => {
                    let _ = app_for_logs.emit("translation-proxy-exit", payload.code);
                    break;
                }
                _ => {}
            }
        }
    });

    // Give the server up to 5s to bind.
    if !wait_for_port(port, Duration::from_secs(5)).await {
        // Best-effort cleanup — child should be dead or dying.
        let _ = child.kill();
        return Err(format!(
            "Translation proxy failed to bind to 127.0.0.1:{} within 5s",
            port
        ));
    }

    let mut guard = state.handle.lock().map_err(|e| e.to_string())?;
    *guard = Some(ProxyHandle {
        child,
        port,
        upstream_base_url: upstream,
    });

    Ok(format!("http://127.0.0.1:{}", port))
}

#[tauri::command]
pub async fn stop_translation_proxy(state: tauri::State<'_, ProxyManager>) -> Result<(), String> {
    state.stop()
}

#[derive(Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub upstream_base_url: Option<String>,
}

#[tauri::command]
pub async fn translation_proxy_status(
    state: tauri::State<'_, ProxyManager>,
) -> Result<ProxyStatus, String> {
    let guard = state.handle.lock().map_err(|e| e.to_string())?;
    if let Some(h) = guard.as_ref() {
        Ok(ProxyStatus {
            running: true,
            port: Some(h.port),
            url: Some(format!("http://127.0.0.1:{}", h.port)),
            upstream_base_url: Some(h.upstream_base_url.clone()),
        })
    } else {
        Ok(ProxyStatus {
            running: false,
            port: None,
            url: None,
            upstream_base_url: None,
        })
    }
}
