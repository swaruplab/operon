use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use super::mcp::MCPServerConfig;

fn default_permission_mode() -> String {
    "full_auto".to_string()
}

fn default_ai_provider() -> String {
    "anthropic".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub font_size: u32,
    pub font_family: String,
    pub tab_size: u32,
    pub word_wrap: bool,
    pub minimap_enabled: bool,
    pub model: String,
    pub max_turns: u32,
    pub max_budget_usd: f64,
    /// Permission level: "full_auto", "safe_mode", or "supervised"
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    pub show_hidden_files: bool,
    pub terminal_font_size: u32,
    #[serde(default)]
    pub setup_completed: bool,
    #[serde(default)]
    pub mcp_servers: Vec<MCPServerConfig>,
    #[serde(default)]
    pub extension_settings: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub last_project_path: Option<String>,
    // ── AI provider (OpenAI-compatible endpoint support) ──
    /// "anthropic" (default) or "custom" for OpenAI-compatible endpoints.
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    /// Base URL for a custom OpenAI-compatible endpoint (e.g. http://localhost:11434/v1).
    /// Operon passes this to Claude Code via ANTHROPIC_BASE_URL.
    #[serde(default)]
    pub custom_base_url: String,
    /// Optional auth token/API key for the custom endpoint.
    /// Passed via ANTHROPIC_AUTH_TOKEN.
    #[serde(default)]
    pub custom_api_key: String,
    /// Model id reported by the custom endpoint (e.g. "qwen2.5-coder:32b").
    #[serde(default)]
    pub custom_model: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 13,
            font_family: "JetBrains Mono".to_string(),
            tab_size: 2,
            word_wrap: false,
            minimap_enabled: true,
            model: "claude-opus-4-20250514".to_string(),
            max_turns: 25,
            max_budget_usd: 5.0,
            permission_mode: "full_auto".to_string(),
            show_hidden_files: false,
            terminal_font_size: 13,
            setup_completed: false,
            mcp_servers: Vec::new(),
            extension_settings: HashMap::new(),
            last_project_path: None,
            ai_provider: "anthropic".to_string(),
            custom_base_url: String::new(),
            custom_api_key: String::new(),
            custom_model: String::new(),
        }
    }
}

pub struct SettingsManager {
    pub settings: Mutex<AppSettings>,
}

impl SettingsManager {
    pub fn new() -> Self {
        // Try to load from disk, fall back to defaults
        let settings = Self::load_from_disk().unwrap_or_default();
        Self {
            settings: Mutex::new(settings),
        }
    }

    pub(crate) fn config_path() -> Option<std::path::PathBuf> {
        Some(crate::platform::config_dir().join("settings.json"))
    }

    fn load_from_disk() -> Option<AppSettings> {
        let path = Self::config_path()?;
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save_to_disk(settings: &AppSettings) -> Result<(), String> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
            std::fs::write(path, data).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, SettingsManager>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
pub async fn update_settings(
    state: tauri::State<'_, SettingsManager>,
    settings: AppSettings,
) -> Result<(), String> {
    SettingsManager::save_to_disk(&settings)?;
    let mut current = state.settings.lock().map_err(|e| e.to_string())?;
    *current = settings;
    Ok(())
}

/// Start platform-native speech recognition.
/// On macOS: uses SFSpeechRecognizer + AVAudioEngine via a Swift subprocess.
/// On other platforms: returns an error (dictation not supported).
#[tauri::command]
pub async fn start_dictation(app_handle: tauri::AppHandle) -> Result<(), String> {
    if !crate::platform::supports_dictation() {
        return Err("Dictation is not supported on this platform".to_string());
    }
    crate::platform::start_dictation_platform(&app_handle)
}

pub(crate) struct DictationProcess {
    pub(crate) stdin: std::process::ChildStdin,
    #[allow(dead_code)]
    pub(crate) pid: u32,
}

pub(crate) static DICTATION_PROCESS: std::sync::Mutex<Option<DictationProcess>> =
    std::sync::Mutex::new(None);

// ── Custom AI endpoint probes ─────────────────────────────────────────────

fn normalize_base_url(base: &str) -> String {
    base.trim().trim_end_matches('/').to_string()
}

/// GET {base}/models on an OpenAI-compatible endpoint and return the list
/// of model ids. Ollama's `/v1/models` (OpenAI compat) and its native
/// `/api/tags` are both accepted — whichever the base URL points at.
#[tauri::command]
pub async fn detect_custom_models(
    base_url: String,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let base = normalize_base_url(&base_url);
    if base.is_empty() {
        return Err("Base URL is empty".to_string());
    }
    // Try OpenAI-compat first
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/models", base);
    let mut req = client.get(&url);
    if let Some(key) = api_key.as_ref() {
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // OpenAI shape: { "data": [ { "id": "..." }, ... ] }
    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
        let mut ids: Vec<String> = data
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        ids.sort();
        return Ok(ids);
    }
    // Ollama native shape: { "models": [ { "name": "..." }, ... ] }
    if let Some(models) = json.get("models").and_then(|v| v.as_array()) {
        let mut ids: Vec<String> = models
            .iter()
            .filter_map(|m| {
                m.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        ids.sort();
        return Ok(ids);
    }
    Err("Unrecognized response shape (no 'data' or 'models' array)".to_string())
}

/// Send a trivial completion to {base}/chat/completions to verify the endpoint
/// is reachable and speaks the OpenAI format. Returns the model echo on success.
#[tauri::command]
pub async fn test_custom_endpoint(
    base_url: String,
    api_key: Option<String>,
    model: Option<String>,
) -> Result<String, String> {
    let base = normalize_base_url(&base_url);
    if base.is_empty() {
        return Err("Base URL is empty".to_string());
    }
    let model_id = model.unwrap_or_default();
    if model_id.is_empty() {
        return Err("Pick a model first".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "model": model_id,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 4,
        "stream": false,
    });
    let url = format!("{}/chat/completions", base);
    let mut req = client.post(&url).json(&body);
    if let Some(key) = api_key.as_ref() {
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "HTTP {} — {}",
            status,
            body_text.chars().take(200).collect::<String>()
        ));
    }
    Ok(format!("OK — {} responded", model_id))
}

/// Stop the currently running dictation process.
#[tauri::command]
pub async fn stop_dictation() -> Result<(), String> {
    use std::io::Write;
    let mut guard = DICTATION_PROCESS.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut process) = *guard {
        let _ = process.stdin.write_all(b"STOP\n");
        let _ = process.stdin.flush();
    }
    *guard = None;
    Ok(())
}
