// Anthropic Models catalog
//
// Fetches the official model list from `GET https://api.anthropic.com/v1/models`,
// caches it to `~/.operon/models_cache.json`, and serves it to the UI so the
// model dropdowns always show the current Anthropic catalog without requiring
// a code/release change every time a new Opus/Sonnet/Haiku ships.
//
// Cache strategy:
//   - Cache file at `<config_dir>/models_cache.json` with `{fetched_at, models}`.
//   - `get_cached_models` returns the cache if present, else a bundled fallback
//     so a brand-new install (no network, no API key) still has a dropdown.
//   - `refresh_models_if_stale` is a no-op if the cache is < 7 days old;
//     otherwise it fetches and rewrites the cache.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CACHE_FILENAME: &str = "models_cache.json";
const CACHE_TTL_SECS: i64 = 7 * 24 * 60 * 60;
const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models?limit=100";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub max_input_tokens: u64,
    #[serde(default)]
    pub max_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsCache {
    pub fetched_at: i64,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<ModelInfo>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    last_id: Option<String>,
}

fn cache_path() -> Option<PathBuf> {
    Some(crate::platform::config_dir().join(CACHE_FILENAME))
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Bundled fallback used when no cache and no API key (or fetch fails).
/// Update this list when shipping a new release so fresh installs see the
/// current models even before the first network call.
fn bundled_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "claude-opus-4-8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
        },
        ModelInfo {
            id: "claude-sonnet-4-6".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            created_at: "2026-02-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 64_000,
        },
        ModelInfo {
            id: "claude-haiku-4-5-20251001".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            created_at: "2025-10-01T00:00:00Z".to_string(),
            max_input_tokens: 200_000,
            max_tokens: 64_000,
        },
    ]
}

fn read_cache() -> Option<ModelsCache> {
    let path = cache_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<ModelsCache>(&data).ok()
}

fn write_cache(cache: &ModelsCache) -> Result<(), String> {
    let path = cache_path().ok_or_else(|| "config dir unavailable".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(())
}

async fn fetch_from_anthropic(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("client build: {}", e))?;

    let mut url = ANTHROPIC_MODELS_URL.to_string();
    let mut all: Vec<ModelInfo> = Vec::new();

    // Page until has_more is false. Anthropic currently returns ~10-15 models
    // total, but loop for safety against future growth.
    loop {
        let resp = client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| format!("network: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Anthropic API {}: {}", status, body));
        }

        let parsed: AnthropicModelsResponse = resp
            .json()
            .await
            .map_err(|e| format!("parse: {}", e))?;

        all.extend(parsed.data);

        if !parsed.has_more {
            break;
        }
        let Some(last) = parsed.last_id else { break };
        url = format!("{}&after_id={}", ANTHROPIC_MODELS_URL, last);
    }

    Ok(all)
}

/// Fetch models from Anthropic's `/v1/models` and update the local cache.
/// Returns the fresh list on success. The caller (frontend) typically calls
/// this from the "Refresh models" button or on startup when the cache is stale.
#[tauri::command]
pub async fn fetch_anthropic_models(api_key: String) -> Result<Vec<ModelInfo>, String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err("API key is empty".to_string());
    }
    let models = fetch_from_anthropic(trimmed).await?;
    let cache = ModelsCache {
        fetched_at: now_unix(),
        models: models.clone(),
    };
    let _ = write_cache(&cache);
    Ok(models)
}

/// Return the cached model list, or the bundled fallback if there's no cache.
/// Always succeeds — the UI can render the dropdown unconditionally.
#[tauri::command]
pub async fn get_cached_models() -> Result<Vec<ModelInfo>, String> {
    if let Some(cache) = read_cache() {
        if !cache.models.is_empty() {
            return Ok(cache.models);
        }
    }
    Ok(bundled_models())
}

/// If the cache is older than 7 days (or missing) AND an API key is provided,
/// fetch a fresh list in the background. No-op otherwise. Safe to call on
/// every app launch; cheap when the cache is warm.
#[tauri::command]
pub async fn refresh_models_if_stale(api_key: Option<String>) -> Result<bool, String> {
    let fresh = read_cache()
        .map(|c| now_unix() - c.fetched_at < CACHE_TTL_SECS)
        .unwrap_or(false);
    if fresh {
        return Ok(false);
    }
    let Some(key) = api_key else { return Ok(false) };
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    match fetch_from_anthropic(trimmed).await {
        Ok(models) => {
            let cache = ModelsCache {
                fetched_at: now_unix(),
                models,
            };
            let _ = write_cache(&cache);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}
