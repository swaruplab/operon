// Portkey gateway provider.
//
// Portkey (https://portkey.ai) is an OpenAI + Anthropic compatible AI gateway.
// Institutions deploy it under their own domain (UCI ZotGPT, etc.); Portkey
// also hosts a multi-tenant cloud version. Operon talks to any Portkey
// deployment via the same code path — only the base URL + virtual API key
// change.
//
// Why this matters: many universities (UCI, and Stanford/MIT/etc. as they
// adopt) have institutional Portkey deployments that bring policy guarantees
// (no training, retention limits, IRB-compatible audit trail) that a personal
// Anthropic key can't provide. Researchers should be able to point Operon at
// "their" gateway with one click.
//
// Architecture:
//   * Bundled fallback preset list ships with the binary (UCI ZotGPT,
//     Portkey Cloud, self-hosted, custom) so a fresh install has useful
//     options before any network call.
//   * Optional hot-fetch from presets/portkey.json in the Operon GitHub repo
//     — any institution can submit a PR adding their gateway. Operon picks it
//     up on next launch.
//   * For session start (in claude.rs), Portkey provider sets
//     ANTHROPIC_BASE_URL + ANTHROPIC_AUTH_TOKEN; Claude Code talks Anthropic
//     `/v1/messages` to Portkey, which routes to whatever backend the model
//     slug specifies.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const PRESETS_CACHE_FILE: &str = "portkey_presets_cache.json";
const PRESETS_TTL_SECS: i64 = 7 * 24 * 60 * 60;
const PRESETS_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/swaruplab/operon/main/presets/portkey.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortkeyPreset {
    /// Stable identifier used in settings (e.g. "uci-zotgpt").
    pub id: String,
    /// Display name shown in the dropdown.
    pub label: String,
    /// `https://...`. Always pre-filled into `portkey_base_url` when selected.
    /// Empty string for `custom`/`self-hosted` so the user fills it in.
    #[serde(default)]
    pub base_url: String,
    /// One-line description shown under the dropdown.
    #[serde(default)]
    pub description: String,
    /// Eligibility text shown as a footnote (who can use this gateway).
    #[serde(default)]
    pub eligibility: String,
    /// URL where users obtain their virtual API key (institutional portal,
    /// signup page, etc.). Rendered as a clickable link in the UI.
    #[serde(default)]
    pub signup_url: String,
    /// URL to the gateway's documentation/privacy notice.
    #[serde(default)]
    pub docs_url: String,
    /// Privacy summary shown as a footnote (one-liner).
    #[serde(default)]
    pub privacy_summary: String,
    /// Suggested model slugs for this gateway (shown as a hint while the
    /// /v1/models call is loading, or if it fails). Empty = no hints.
    #[serde(default)]
    pub suggested_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetsManifest {
    presets: Vec<PortkeyPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetsCache {
    fetched_at: i64,
    presets: Vec<PortkeyPreset>,
}

fn cache_path() -> Option<PathBuf> {
    Some(crate::platform::config_dir().join(PRESETS_CACHE_FILE))
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The bundled fallback preset list. Updating this list updates the
/// out-of-box experience for fresh installs. Long-tail of presets lives in
/// `presets/portkey.json` in the repo and gets merged in via the hot fetcher.
fn bundled_presets() -> Vec<PortkeyPreset> {
    vec![
        PortkeyPreset {
            id: "uci-zotgpt".to_string(),
            label: "UCI ZotGPT Gateway".to_string(),
            base_url: "https://api.zotgpt.uci.edu/v1".to_string(),
            description: "UC Irvine's institutional AI gateway (powered by Portkey).".to_string(),
            eligibility:
                "UCI faculty, staff, graduate students; PI-sponsored undergraduates.".to_string(),
            signup_url: "https://portal.azureapi.zotgpt.uci.edu".to_string(),
            docs_url: "https://zotgpt.uci.edu/services/gateway".to_string(),
            privacy_summary:
                "12-month retention, no training, P3 compliant, IRB-relevant audit trail."
                    .to_string(),
            suggested_models: vec![
                "@zotgpt-api-bedrock/us.anthropic.claude-opus-4-7".to_string(),
                "@zotgpt-api-bedrock/us.anthropic.claude-sonnet-4-5".to_string(),
                "@openai-prod/gpt-5.5".to_string(),
            ],
        },
        PortkeyPreset {
            id: "portkey-cloud".to_string(),
            label: "Portkey Cloud".to_string(),
            base_url: "https://api.portkey.ai/v1".to_string(),
            description: "Portkey's multi-tenant hosted gateway. Pay-as-you-go.".to_string(),
            eligibility: "Anyone with a Portkey account.".to_string(),
            signup_url: "https://app.portkey.ai/signup".to_string(),
            docs_url: "https://portkey.ai/docs".to_string(),
            privacy_summary:
                "Portkey's data handling policy applies. Configure providers in the Portkey dashboard."
                    .to_string(),
            suggested_models: vec![],
        },
        PortkeyPreset {
            id: "self-hosted".to_string(),
            label: "Self-hosted Portkey".to_string(),
            base_url: String::new(),
            description: "A Portkey gateway your lab or institution runs.".to_string(),
            eligibility: "Whoever your gateway admin permits.".to_string(),
            signup_url: String::new(),
            docs_url: "https://github.com/Portkey-AI/gateway".to_string(),
            privacy_summary: "Whatever your gateway operator publishes.".to_string(),
            suggested_models: vec![],
        },
        PortkeyPreset {
            id: "custom".to_string(),
            label: "Other institutional gateway".to_string(),
            base_url: String::new(),
            description: "Any Portkey-compatible endpoint — paste your gateway's URL."
                .to_string(),
            eligibility: String::new(),
            signup_url: String::new(),
            docs_url: String::new(),
            privacy_summary: "Whatever your gateway operator publishes.".to_string(),
            suggested_models: vec![],
        },
    ]
}

fn read_cache() -> Option<PresetsCache> {
    let path = cache_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_cache(cache: &PresetsCache) -> Result<(), String> {
    let path = cache_path().ok_or_else(|| "config dir unavailable".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())
}

/// Merge the bundled list with the fetched manifest. Fetched presets win on
/// `id` collision — that's how a hotfix in `presets/portkey.json` can update
/// an existing entry (e.g. UCI's base URL changes) without an Operon release.
fn merge_presets(bundled: Vec<PortkeyPreset>, fetched: Vec<PortkeyPreset>) -> Vec<PortkeyPreset> {
    let mut by_id: std::collections::HashMap<String, PortkeyPreset> =
        bundled.into_iter().map(|p| (p.id.clone(), p)).collect();
    for p in fetched {
        by_id.insert(p.id.clone(), p);
    }
    // Stable ordering: institutional presets first, then cloud/self-hosted/custom.
    // (Sort by label as a sane default; the JSON can reorder later if needed.)
    let mut out: Vec<PortkeyPreset> = by_id.into_values().collect();
    out.sort_by(|a, b| {
        let order = |p: &PortkeyPreset| match p.id.as_str() {
            "custom" => 99,
            "self-hosted" => 98,
            "portkey-cloud" => 97,
            _ => 0,
        };
        let oa = order(a);
        let ob = order(b);
        if oa == ob {
            a.label.cmp(&b.label)
        } else {
            oa.cmp(&ob)
        }
    });
    out
}

/// Return the merged preset list. Always succeeds — falls back to bundled on
/// any cache/network problem. The UI can render the dropdown unconditionally.
#[tauri::command]
pub async fn list_portkey_presets() -> Result<Vec<PortkeyPreset>, String> {
    let fetched = read_cache().map(|c| c.presets).unwrap_or_default();
    Ok(merge_presets(bundled_presets(), fetched))
}

async fn fetch_presets_from_github() -> Result<Vec<PortkeyPreset>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("client build: {}", e))?;

    let resp = client
        .get(PRESETS_MANIFEST_URL)
        .send()
        .await
        .map_err(|e| format!("network: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("manifest HTTP {}", resp.status()));
    }
    let manifest: PresetsManifest = resp
        .json()
        .await
        .map_err(|e| format!("manifest parse: {}", e))?;
    Ok(manifest.presets)
}

/// Refresh the presets cache from GitHub. No-op if the cache is < 7 days old.
/// Silent on failure — the bundled list keeps working.
#[tauri::command]
pub async fn refresh_portkey_presets() -> Result<bool, String> {
    let fresh = read_cache()
        .map(|c| now_unix() - c.fetched_at < PRESETS_TTL_SECS)
        .unwrap_or(false);
    if fresh {
        return Ok(false);
    }
    match fetch_presets_from_github().await {
        Ok(presets) => {
            let cache = PresetsCache {
                fetched_at: now_unix(),
                presets,
            };
            let _ = write_cache(&cache);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

// ─── Model catalog (per-gateway) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortkeyModel {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub owned_by: String,
}

#[derive(Debug, Deserialize)]
struct PortkeyModelsResponse {
    data: Vec<PortkeyModel>,
}

/// Fetch the model catalog from a configured Portkey gateway. Portkey
/// exposes `/v1/models` in OpenAI's format; the response shape is
/// `{data: [{id, owned_by, ...}]}`. Bearer auth with the virtual key.
#[tauri::command]
pub async fn fetch_portkey_models(
    base_url: String,
    api_key: String,
) -> Result<Vec<PortkeyModel>, String> {
    let base = base_url.trim_end_matches('/');
    if base.is_empty() || api_key.trim().is_empty() {
        return Err("base URL and API key are required".to_string());
    }
    let url = format!("{}/models", base);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("client build: {}", e))?;

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .header("x-portkey-api-key", api_key.trim())
        .send()
        .await
        .map_err(|e| format!("network: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Portkey {} {}: {}", url, status, body));
    }
    let parsed: PortkeyModelsResponse = resp.json().await.map_err(|e| format!("parse: {}", e))?;
    Ok(parsed.data)
}
