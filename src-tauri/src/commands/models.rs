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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffortCapability {
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub low: CapabilitySupport,
    #[serde(default)]
    pub medium: CapabilitySupport,
    #[serde(default)]
    pub high: CapabilitySupport,
    // Canonical order is low < medium < high < xhigh < max: `xhigh` sits
    // BETWEEN `high` and `max`. Declared in that order so the struct, the
    // serialized cache and `src/lib/models.ts` all read the same way.
    #[serde(default)]
    pub xhigh: CapabilitySupport,
    #[serde(default)]
    pub max: CapabilitySupport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySupport {
    #[serde(default)]
    pub supported: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub effort: EffortCapability,
}

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
    #[serde(default)]
    pub capabilities: ModelCapabilities,
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
    let yes = CapabilitySupport { supported: true };
    let all_effort = EffortCapability {
        supported: true,
        low: yes.clone(),
        medium: yes.clone(),
        high: yes.clone(),
        xhigh: yes.clone(),
        max: yes.clone(),
    };
    let sonnet_effort = EffortCapability {
        supported: true,
        low: yes.clone(),
        medium: yes.clone(),
        high: yes.clone(),
        xhigh: CapabilitySupport::default(),
        max: CapabilitySupport::default(),
    };
    let no_effort = EffortCapability::default();
    vec![
        // Anthropic's most capable widely released model, and newer than Opus
        // 5. Operon's default deliberately stays Opus 5 (see `DEFAULT_MODEL` in
        // settings.rs) — Fable is offered, not forced.
        //
        // Fable gets its own tier in the dropdowns (`tierOf` in
        // src/lib/models.ts), and both dropdowns render that tier first, so
        // `created_at` does not decide its position against Opus 5 — it orders
        // models WITHIN a tier, which is what will place a future Fable release
        // above this one.
        ModelInfo {
            id: "claude-fable-5-1".to_string(),
            display_name: "Claude Fable 5.1".to_string(),
            created_at: "2026-08-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: ModelCapabilities {
                effort: all_effort.clone(),
            },
        },
        ModelInfo {
            id: "claude-opus-5".to_string(),
            display_name: "Claude Opus 5".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            // Operon's default model — see `AppSettings::default()` in
            // settings.rs. Full effort range; the shipped default is `high`.
            capabilities: ModelCapabilities {
                effort: all_effort.clone(),
            },
        },
        ModelInfo {
            id: "claude-opus-4-8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: ModelCapabilities {
                effort: all_effort.clone(),
            },
        },
        ModelInfo {
            id: "claude-sonnet-5".to_string(),
            display_name: "Claude Sonnet 5".to_string(),
            created_at: "2026-06-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            // Sonnet 5 is the first Sonnet-tier model to support the full effort
            // range, including `xhigh` and `max` — same capabilities as Opus.
            capabilities: ModelCapabilities { effort: all_effort },
        },
        ModelInfo {
            id: "claude-sonnet-4-6".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            created_at: "2026-02-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 64_000,
            capabilities: ModelCapabilities {
                effort: sonnet_effort,
            },
        },
        // Current Anthropic model ids are complete as-is and carry no date
        // suffix; the dated `claude-haiku-4-5-20251001` Operon used to ship is
        // retired and is stripped from stale caches by [`RETIRED_MODEL_IDS`].
        ModelInfo {
            id: "claude-haiku-4-5".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            created_at: "2025-10-01T00:00:00Z".to_string(),
            max_input_tokens: 200_000,
            max_tokens: 64_000,
            capabilities: ModelCapabilities { effort: no_effort },
        },
    ]
}

/// Model ids Operon used to ship that Anthropic has since retired, each mapped
/// to the id that supersedes it — the same model under its current name, or its
/// successor where the model itself is gone. Current Anthropic ids are complete
/// as they stand and never take a date suffix, so a dated id can only have come
/// from an older Operon release (or from the `models_cache.json` one wrote).
///
/// This is the single source for BOTH halves of a retirement: `drop_retired_ids`
/// evicts the id from the catalog, and `migrate_settings` (settings.rs) rewrites
/// it wherever a user has one stored. Retiring another id is one line here.
pub(crate) const RETIRED_MODEL_IDS: &[(&str, &str)] = &[
    // Pure rename — same model, the date suffix was never part of the id.
    ("claude-haiku-4-5-20251001", "claude-haiku-4-5"),
    // Genuinely retired; mapped to the successor in the same tier.
    ("claude-opus-4-20250514", "claude-opus-5"),
    ("claude-sonnet-4-20250514", "claude-sonnet-4-6"),
];

/// Drop entries from `models` whose id is retired **and** whose replacement
/// will be in the final list — either already in `models` or about to be
/// unioned in from `incoming` (the bundled catalog).
///
/// Without this, a returning user whose `models_cache.json` still holds
/// `claude-haiku-4-5-20251001` gets BOTH it and `claude-haiku-4-5` unioned
/// together: two identical-looking "Claude Haiku 4.5" rows in every dropdown,
/// one of them a dead id the CLI would reject — and it's the dead one a
/// returning user may still have selected.
///
/// A retired id whose replacement is nowhere to be found is deliberately KEPT:
/// dropping it could empty the dropdown, which is worse than a stale row.
fn drop_retired_ids(models: &mut Vec<ModelInfo>, incoming: &[ModelInfo], retired: &[(&str, &str)]) {
    let present: std::collections::HashSet<String> = models
        .iter()
        .chain(incoming.iter())
        .map(|m| m.id.clone())
        .collect();
    models.retain(|m| match retired.iter().find(|(old, _)| *old == m.id) {
        Some((_, replacement)) => !present.contains(*replacement),
        None => true,
    });
}

/// Anthropic's `GET /v1/models` returns id / display_name / created_at but NOT
/// the per-model effort ("thinking") capabilities or token limits Operon needs
/// to drive the effort selector. Without this overlay every fetched model comes
/// back with `effort.supported == false`, so the moment a user sets an API key
/// (which triggers a fetch and overwrites the cache) the effort dropdown
/// silently disappears — even for models that clearly support it (Opus, Sonnet
/// 5). This re-applies the curated capabilities from `bundled_models()` by exact
/// id, backfills token limits / display names the fetch left empty, and unions
/// in any bundled model the fetch didn't return (e.g. a brand-new model not yet
/// listed in `/v1/models`) so a freshly shipped model like Sonnet 5 is always
/// selectable with its full effort range regardless of the fetch path.
fn enrich_fetched_models(mut fetched: Vec<ModelInfo>) -> Vec<ModelInfo> {
    let bundled = bundled_models();
    // Before overlaying or unioning: evict ids Anthropic has retired, so a
    // stale cache can't contribute a duplicate row for a model the bundled
    // catalog already carries under its current id.
    drop_retired_ids(&mut fetched, &bundled, RETIRED_MODEL_IDS);
    for m in fetched.iter_mut() {
        let Some(b) = bundled.iter().find(|b| b.id == m.id) else {
            continue;
        };
        // Only overlay when the fetch gave us nothing — never clobber real caps
        // a future API version might start returning.
        if !m.capabilities.effort.supported {
            m.capabilities = b.capabilities.clone();
        }
        if m.max_tokens == 0 {
            m.max_tokens = b.max_tokens;
        }
        if m.max_input_tokens == 0 {
            m.max_input_tokens = b.max_input_tokens;
        }
        if m.display_name.is_empty() {
            m.display_name = b.display_name.clone();
        }
    }
    for b in bundled {
        if !fetched.iter().any(|m| m.id == b.id) {
            fetched.push(b);
        }
    }
    fetched
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

        let parsed: AnthropicModelsResponse =
            resp.json().await.map_err(|e| format!("parse: {}", e))?;

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
    let models = enrich_fetched_models(fetch_from_anthropic(trimmed).await?);
    let cache = ModelsCache {
        fetched_at: now_unix(),
        models: models.clone(),
    };
    let _ = write_cache(&cache);
    Ok(models)
}

/// Synchronous accessor for the cached model list (with bundled fallback).
/// Used by non-Tauri callers like the claude command assembler that need to
/// look up a model's capabilities without going through invoke.
///
/// The cache is enriched on READ, not just on fetch. A model shipped in a new
/// release (e.g. `claude-opus-5`) would otherwise be invisible to anyone with an
/// existing `models_cache.json`: absent from both dropdowns, and — worse —
/// unknown to [`model_supports_effort_level`], which would silently drop
/// `--effort` from every command. That state is permanent for subscription
/// users, since `refresh_models_if_stale` needs an API key to ever rewrite the
/// cache and they don't have one.
pub fn cached_models_sync() -> Vec<ModelInfo> {
    read_cache()
        .map(|c| enrich_fetched_models(c.models))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(bundled_models)
}

/// Return true if `model_id` supports the given effort level. Models not
/// found in cache fall through to false → the caller silently omits the
/// --effort flag rather than failing.
pub fn model_supports_effort_level(model_id: &str, level: &str) -> bool {
    let models = cached_models_sync();
    let Some(m) = models.iter().find(|m| m.id == model_id) else {
        return false;
    };
    if !m.capabilities.effort.supported {
        return false;
    }
    match level {
        "low" => m.capabilities.effort.low.supported,
        "medium" => m.capabilities.effort.medium.supported,
        "high" => m.capabilities.effort.high.supported,
        "xhigh" => m.capabilities.effort.xhigh.supported,
        "max" => m.capabilities.effort.max.supported,
        _ => false,
    }
}

/// Return the cached model list, or the bundled fallback if there's no cache.
/// Always succeeds — the UI can render the dropdown unconditionally.
#[tauri::command]
pub async fn get_cached_models() -> Result<Vec<ModelInfo>, String> {
    Ok(cached_models_sync())
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
                models: enrich_fetched_models(models),
            };
            let _ = write_cache(&cache);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_5_is_bundled_with_full_effort() {
        let models = bundled_models();
        let o5 = models
            .iter()
            .find(|m| m.id == "claude-opus-5")
            .expect("Opus 5 must be in the bundled catalog — it is the shipped default model");
        assert!(o5.capabilities.effort.supported);
        // The shipped default is `high`; the rest of the range must be
        // selectable so a user pinning "max"/"xhigh" isn't silently downgraded.
        assert!(o5.capabilities.effort.low.supported);
        assert!(o5.capabilities.effort.medium.supported);
        assert!(o5.capabilities.effort.high.supported);
        assert!(o5.capabilities.effort.max.supported);
        assert!(o5.capabilities.effort.xhigh.supported);
    }

    #[test]
    fn fable_5_1_is_bundled_first_with_full_effort() {
        let models = bundled_models();
        assert_eq!(
            models[0].id, "claude-fable-5-1",
            "Fable 5.1 leads the bundled catalog"
        );
        let f = &models[0];
        assert_eq!(f.display_name, "Claude Fable 5.1");
        assert_eq!(f.max_input_tokens, 1_000_000);
        assert_eq!(f.max_tokens, 128_000);
        assert!(f.capabilities.effort.supported);
        assert!(f.capabilities.effort.low.supported);
        assert!(f.capabilities.effort.medium.supported);
        assert!(f.capabilities.effort.high.supported);
        assert!(f.capabilities.effort.xhigh.supported);
        assert!(f.capabilities.effort.max.supported);
        // The claude command assembler gates `--effort` on this.
        assert!(model_supports_effort_level("claude-fable-5-1", "xhigh"));
        assert!(model_supports_effort_level("claude-fable-5-1", "max"));
    }

    #[test]
    fn fable_5_1_sorts_above_opus_5() {
        // The frontend orders each tier by `created_at` DESC as a plain string
        // compare, so Fable's date has to be strictly greater than Opus 5's or
        // the newer model lands below the older one in the dropdown.
        let models = bundled_models();
        let fable = models.iter().find(|m| m.id == "claude-fable-5-1").unwrap();
        let opus5 = models.iter().find(|m| m.id == "claude-opus-5").unwrap();
        assert!(
            fable.created_at.as_str() > opus5.created_at.as_str(),
            "{} must sort above {}",
            fable.created_at,
            opus5.created_at
        );
    }

    #[test]
    fn haiku_4_5_uses_the_undated_id_and_supports_no_effort() {
        let models = bundled_models();
        assert!(
            !models.iter().any(|m| m.id.contains("20251001")),
            "current Anthropic ids carry no date suffix"
        );
        let h = models
            .iter()
            .find(|m| m.id == "claude-haiku-4-5")
            .expect("Haiku 4.5 bundled under its undated id");
        assert_eq!(h.display_name, "Claude Haiku 4.5");
        assert_eq!(h.created_at, "2025-10-01T00:00:00Z");
        assert_eq!(h.max_input_tokens, 200_000);
        assert_eq!(h.max_tokens, 64_000);
        assert!(!h.capabilities.effort.supported);
        for level in ["low", "medium", "high", "xhigh", "max"] {
            assert!(
                !model_supports_effort_level("claude-haiku-4-5", level),
                "Haiku 4.5 supports no effort level, got {level}"
            );
        }
    }

    #[test]
    fn opus_5_default_still_has_every_effort_level() {
        // Fable 5.1 shipping alongside must not disturb the default model.
        for level in ["low", "medium", "high", "xhigh", "max"] {
            assert!(
                model_supports_effort_level("claude-opus-5", level),
                "Opus 5 must keep {level}"
            );
        }
    }

    #[test]
    fn a_cache_with_the_retired_haiku_id_enriches_to_one_clean_haiku() {
        // The upgrade shape: an existing models_cache.json still holds the dated
        // id. Union alone would produce two identical-looking "Claude Haiku 4.5"
        // rows, and the dead one is what a returning user may have selected.
        let cached = vec![ModelInfo {
            id: "claude-haiku-4-5-20251001".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            created_at: "2025-10-01T00:00:00Z".to_string(),
            max_input_tokens: 200_000,
            max_tokens: 64_000,
            capabilities: ModelCapabilities::default(),
        }];
        let enriched = enrich_fetched_models(cached);
        let haikus: Vec<&ModelInfo> = enriched
            .iter()
            .filter(|m| m.display_name == "Claude Haiku 4.5")
            .collect();
        assert_eq!(haikus.len(), 1, "exactly one Haiku row survives");
        assert_eq!(haikus[0].id, "claude-haiku-4-5");
        assert!(!enriched.iter().any(|m| m.id.contains("20251001")));
    }

    #[test]
    fn every_retired_id_is_evicted_when_its_replacement_ships() {
        // The table drives both halves of a retirement, so each entry must
        // actually resolve against the bundled catalog — a typo'd replacement
        // would silently keep the dead id in every dropdown forever.
        let bundled = bundled_models();
        for (old, replacement) in RETIRED_MODEL_IDS {
            assert!(
                bundled.iter().any(|m| m.id == *replacement),
                "{old} maps to {replacement}, which is not in the bundled catalog"
            );
            assert!(
                !bundled.iter().any(|m| m.id == *old),
                "{old} is retired but still shipped in bundled_models()"
            );
            let cached = vec![ModelInfo {
                id: (*old).to_string(),
                display_name: "stale".to_string(),
                created_at: "2025-01-01T00:00:00Z".to_string(),
                max_input_tokens: 0,
                max_tokens: 0,
                capabilities: ModelCapabilities::default(),
            }];
            let enriched = enrich_fetched_models(cached);
            assert!(
                !enriched.iter().any(|m| m.id == *old),
                "{old} survived enrichment"
            );
            assert!(enriched.iter().any(|m| m.id == *replacement));
        }
    }

    #[test]
    fn a_retired_id_is_kept_when_its_replacement_is_missing() {
        // Never silently empty a dropdown: if the replacement is in neither the
        // cache nor the bundled catalog, the stale row is better than nothing.
        let mut models = vec![ModelInfo {
            id: "claude-opus-4-8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: ModelCapabilities::default(),
        }];
        drop_retired_ids(&mut models, &[], &[("claude-opus-4-8", "claude-opus-4-9")]);
        assert_eq!(models.len(), 1, "no replacement anywhere → keep the entry");

        // ...and it IS dropped once the replacement is on offer.
        let replacement = vec![ModelInfo {
            id: "claude-opus-4-9".to_string(),
            display_name: "Claude Opus 4.9".to_string(),
            created_at: "2026-05-02T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: ModelCapabilities::default(),
        }];
        drop_retired_ids(
            &mut models,
            &replacement,
            &[("claude-opus-4-8", "claude-opus-4-9")],
        );
        assert!(
            models.is_empty(),
            "replacement present → retired id evicted"
        );
    }

    #[test]
    fn stale_cache_still_yields_opus_5() {
        // A user upgrading from an older release has a models_cache.json written
        // before Opus 5 existed. Enrichment-on-read must union it back in, or the
        // default model is missing from the dropdowns and --effort is dropped.
        let stale = vec![ModelInfo {
            id: "claude-opus-4-8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: ModelCapabilities::default(),
        }];
        let enriched = enrich_fetched_models(stale);
        let o5 = enriched
            .iter()
            .find(|m| m.id == "claude-opus-5")
            .expect("Opus 5 unioned into a cache that predates it");
        assert!(o5.capabilities.effort.high.supported);
    }

    #[test]
    fn sonnet_5_is_bundled_with_full_effort() {
        let models = bundled_models();
        let s5 = models
            .iter()
            .find(|m| m.id == "claude-sonnet-5")
            .expect("sonnet 5 must be bundled");
        assert!(s5.capabilities.effort.supported);
        assert!(s5.capabilities.effort.high.supported);
        assert!(s5.capabilities.effort.max.supported);
        assert!(s5.capabilities.effort.xhigh.supported);
        assert_eq!(s5.max_input_tokens, 1_000_000);
        assert_eq!(s5.max_tokens, 128_000);
        // The claude command assembler gates `--effort` on this.
        assert!(model_supports_effort_level("claude-sonnet-5", "max"));
        assert!(model_supports_effort_level("claude-sonnet-5", "xhigh"));
    }

    #[test]
    fn sonnet_4_6_still_caps_at_high() {
        // Guard against accidentally giving 4.6 max/xhigh, which the CLI would reject.
        assert!(model_supports_effort_level("claude-sonnet-4-6", "high"));
        assert!(!model_supports_effort_level("claude-sonnet-4-6", "max"));
        assert!(!model_supports_effort_level("claude-sonnet-4-6", "xhigh"));
    }

    #[test]
    fn enrich_overlays_effort_onto_fetched_model_with_empty_caps() {
        // Simulate what /v1/models actually returns: id + name, no capabilities.
        let fetched = vec![ModelInfo {
            id: "claude-sonnet-5".to_string(),
            display_name: "Claude Sonnet 5".to_string(),
            created_at: "2026-06-01T00:00:00Z".to_string(),
            max_input_tokens: 0,
            max_tokens: 0,
            capabilities: ModelCapabilities::default(),
        }];
        let enriched = enrich_fetched_models(fetched);
        let s5 = enriched.iter().find(|m| m.id == "claude-sonnet-5").unwrap();
        assert!(
            s5.capabilities.effort.supported,
            "effort re-applied from bundled after a caps-less fetch"
        );
        assert!(s5.capabilities.effort.max.supported);
        assert_eq!(s5.max_tokens, 128_000, "token limit backfilled");
        assert_eq!(s5.max_input_tokens, 1_000_000);
    }

    #[test]
    fn enrich_unions_in_bundled_models_missing_from_fetch() {
        // A fetch that only returns opus must still yield a selectable Sonnet 5.
        let fetched = vec![ModelInfo {
            id: "claude-opus-4-8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: ModelCapabilities::default(),
        }];
        let enriched = enrich_fetched_models(fetched);
        assert!(
            enriched.iter().any(|m| m.id == "claude-sonnet-5"),
            "sonnet 5 unioned in from bundled"
        );
        let s5 = enriched.iter().find(|m| m.id == "claude-sonnet-5").unwrap();
        assert!(s5.capabilities.effort.max.supported);
    }

    #[test]
    fn enrich_preserves_real_caps_a_future_fetch_might_return() {
        let mut caps = ModelCapabilities::default();
        caps.effort.supported = true;
        caps.effort.low.supported = true;
        let fetched = vec![ModelInfo {
            id: "claude-opus-4-8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            max_input_tokens: 1_000_000,
            max_tokens: 128_000,
            capabilities: caps,
        }];
        let enriched = enrich_fetched_models(fetched);
        let opus = enriched.iter().find(|m| m.id == "claude-opus-4-8").unwrap();
        // Fetched caps (only low) preserved; bundled all-effort NOT overlaid.
        assert!(opus.capabilities.effort.supported);
        assert!(opus.capabilities.effort.low.supported);
        assert!(!opus.capabilities.effort.max.supported);
    }
}
