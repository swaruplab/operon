use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Emitter;
use std::process::{Child, Command, Stdio};

// ── Data Structures ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub path: String,
    pub contributions: ExtContributions,
    pub publisher: String,
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtContributions {
    pub themes: Vec<ThemeContribution>,
    pub snippets: Vec<SnippetContribution>,
    pub grammars: Vec<GrammarContribution>,
    pub languages: Vec<LanguageContribution>,
    pub configuration: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeContribution {
    pub label: String,
    pub ui_theme: String, // "vs-dark", "vs", "hc-black"
    pub path: String,     // relative path within extension dir
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetContribution {
    pub language: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrammarContribution {
    pub language: String,
    pub scope_name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageContribution {
    pub id: String,
    pub extensions: Vec<String>,
    pub aliases: Vec<String>,
}

// ── Open VSX API Response Types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub offset: u32,
    pub total_size: u32,
    pub extensions: Vec<ExtensionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub url: Option<String>,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub timestamp: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub verified: Option<bool>,
    pub deprecated: Option<bool>,
    pub download_count: Option<u64>,
    pub average_rating: Option<f64>,
    pub review_count: Option<u32>,
    pub files: Option<ExtensionFiles>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionFiles {
    pub download: Option<String>,
    pub icon: Option<String>,
    pub manifest: Option<String>,
    pub readme: Option<String>,
    pub changelog: Option<String>,
    pub license: Option<String>,
    pub signature: Option<String>,
    pub sha256: Option<String>,
    #[serde(rename = "publicKey")]
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionDetail {
    pub url: Option<String>,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub timestamp: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub verified: Option<bool>,
    pub deprecated: Option<bool>,
    pub download_count: Option<u64>,
    pub average_rating: Option<f64>,
    pub review_count: Option<u32>,
    pub files: Option<ExtensionFiles>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub bugs: Option<String>,
    pub engines: Option<HashMap<String, String>>,
    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub extension_kind: Option<Vec<String>>,
    pub preview: Option<bool>,
    pub pre_release: Option<bool>,
    pub published_by: Option<Publisher>,
    pub dependencies: Option<Vec<ExtensionRef>>,
    pub bundled_extensions: Option<Vec<ExtensionRef>>,
    pub all_versions: Option<HashMap<String, String>>,
    pub reviews_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Publisher {
    pub login_name: Option<String>,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionRef {
    pub namespace: Option<String>,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub user: Option<Publisher>,
    pub timestamp: Option<String>,
    pub rating: Option<u8>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceDetail {
    pub name: String,
    pub extensions: Option<HashMap<String, String>>,
    pub verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub level: String, // "full", "lsp", "partial", "not_compatible"
    pub supported: Vec<String>,
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    pub extension_id: String,
    pub stage: String,
    pub percent: u32,
}

// ── Extension Manager ────────────────────────────────────────────────────

pub struct LanguageServerHandle {
    pub extension_id: String,
    pub server_id: String,
    pub child: Mutex<Child>,
    pub languages: Vec<String>,
}

pub struct ExtensionManager {
    pub registry: Mutex<HashMap<String, InstalledExtension>>,
    pub extensions_dir: PathBuf,
    pub running_servers: Mutex<HashMap<String, LanguageServerHandle>>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        let extensions_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("operon")
            .join("extensions");
        std::fs::create_dir_all(&extensions_dir).ok();

        let registry = Self::load_registry(&extensions_dir);
        Self {
            registry: Mutex::new(registry),
            extensions_dir,
            running_servers: Mutex::new(HashMap::new()),
        }
    }

    fn registry_path(extensions_dir: &PathBuf) -> PathBuf {
        extensions_dir.join("registry.json")
    }

    fn load_registry(extensions_dir: &PathBuf) -> HashMap<String, InstalledExtension> {
        let path = Self::registry_path(extensions_dir);
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn save_registry(
        extensions_dir: &PathBuf,
        registry: &HashMap<String, InstalledExtension>,
    ) -> Result<(), String> {
        let path = Self::registry_path(extensions_dir);
        let data = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ── Helper: Parse package.json contributions ─────────────────────────────

fn parse_contributions(package_json: &serde_json::Value) -> ExtContributions {
    let contributes = package_json.get("contributes").cloned().unwrap_or_default();
    let mut contribs = ExtContributions::default();

    // Themes
    if let Some(themes) = contributes.get("themes").and_then(|t| t.as_array()) {
        for theme in themes {
            if let (Some(label), Some(path)) = (
                theme.get("label").and_then(|l| l.as_str()),
                theme.get("path").and_then(|p| p.as_str()),
            ) {
                let ui_theme = theme
                    .get("uiTheme")
                    .and_then(|u| u.as_str())
                    .unwrap_or("vs-dark")
                    .to_string();
                contribs.themes.push(ThemeContribution {
                    label: label.to_string(),
                    ui_theme,
                    path: path.to_string(),
                });
            }
        }
    }

    // Snippets
    if let Some(snippets) = contributes.get("snippets").and_then(|s| s.as_array()) {
        for snippet in snippets {
            if let (Some(language), Some(path)) = (
                snippet.get("language").and_then(|l| l.as_str()),
                snippet.get("path").and_then(|p| p.as_str()),
            ) {
                contribs.snippets.push(SnippetContribution {
                    language: language.to_string(),
                    path: path.to_string(),
                });
            }
        }
    }

    // Grammars
    if let Some(grammars) = contributes.get("grammars").and_then(|g| g.as_array()) {
        for grammar in grammars {
            if let (Some(scope_name), Some(path)) = (
                grammar.get("scopeName").and_then(|s| s.as_str()),
                grammar.get("path").and_then(|p| p.as_str()),
            ) {
                let language = grammar
                    .get("language")
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string();
                contribs.grammars.push(GrammarContribution {
                    language,
                    scope_name: scope_name.to_string(),
                    path: path.to_string(),
                });
            }
        }
    }

    // Languages
    if let Some(languages) = contributes.get("languages").and_then(|l| l.as_array()) {
        for lang in languages {
            if let Some(id) = lang.get("id").and_then(|i| i.as_str()) {
                let extensions = lang
                    .get("extensions")
                    .and_then(|e| e.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let aliases = lang
                    .get("aliases")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                contribs.languages.push(LanguageContribution {
                    id: id.to_string(),
                    extensions,
                    aliases,
                });
            }
        }
    }

    // Configuration
    if let Some(config) = contributes.get("configuration") {
        contribs.configuration = Some(config.clone());
    }

    contribs
}

fn analyze_compatibility(package_json: &serde_json::Value) -> CompatibilityReport {
    let contributes = package_json.get("contributes");
    let empty = serde_json::Value::Object(serde_json::Map::new());
    let contributes = contributes.unwrap_or(&empty);

    let supported_keys = [
        "themes",
        "iconThemes",
        "snippets",
        "languages",
        "grammars",
        "configuration",
        "jsonValidation",
        "keybindings",
    ];
    let unsupported_keys = [
        "commands",
        "menus",
        "views",
        "viewsContainers",
        "walkthroughs",
        "problemMatchers",
        "submenus",
    ];

    let mut supported = Vec::new();
    let mut unsupported = Vec::new();

    for key in &supported_keys {
        if contributes.get(*key).is_some() {
            supported.push(key.to_string());
        }
    }
    for key in &unsupported_keys {
        if contributes.get(*key).is_some() {
            unsupported.push(key.to_string());
        }
    }

    let has_languages = contributes.get("languages").is_some();
    let level = if !supported.is_empty() && unsupported.is_empty() {
        "full".to_string()
    } else if !supported.is_empty() && has_languages {
        "lsp".to_string()
    } else if !supported.is_empty() && !unsupported.is_empty() {
        "partial".to_string()
    } else {
        "not_compatible".to_string()
    };

    CompatibilityReport {
        level,
        supported,
        unsupported,
    }
}

// ── Open VSX API Constants ───────────────────────────────────────────────

const OPEN_VSX_BASE: &str = "https://open-vsx.org/api";

// ── Tauri Commands: Open VSX API ─────────────────────────────────────────

#[tauri::command]
pub async fn search_extensions(
    query: String,
    category: Option<String>,
    offset: Option<u32>,
    size: Option<u32>,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<SearchResult, String> {
    let client = reqwest::Client::new();
    let mut params: Vec<(&str, String)> = vec![
        ("query", query),
        ("offset", (offset.unwrap_or(0)).to_string()),
        ("size", (size.unwrap_or(18)).to_string()),
    ];
    if let Some(cat) = &category {
        params.push(("category", cat.clone()));
    }
    if let Some(sort) = &sort_by {
        params.push(("sortBy", sort.clone()));
    }
    if let Some(order) = &sort_order {
        params.push(("sortOrder", order.clone()));
    }

    let resp = client
        .get(format!("{OPEN_VSX_BASE}/-/search"))
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("Search request failed: {}", e))?;

    let result: SearchResult = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse search results: {}", e))?;

    Ok(result)
}

#[tauri::command]
pub async fn get_extension_details(
    namespace: String,
    name: String,
) -> Result<ExtensionDetail, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{OPEN_VSX_BASE}/{namespace}/{name}"))
        .send()
        .await
        .map_err(|e| format!("Detail request failed: {}", e))?;

    let detail: ExtensionDetail = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse extension detail: {}", e))?;

    Ok(detail)
}

#[tauri::command]
pub async fn get_extension_manifest(
    namespace: String,
    name: String,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    // First get the detail to find the manifest URL
    let detail_resp = client
        .get(format!("{OPEN_VSX_BASE}/{namespace}/{name}"))
        .send()
        .await
        .map_err(|e| format!("Detail request failed: {}", e))?;

    let detail: ExtensionDetail = detail_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse detail: {}", e))?;

    let manifest_url = detail
        .files
        .as_ref()
        .and_then(|f| f.manifest.as_ref())
        .ok_or("No manifest URL available")?;

    let manifest_resp = client
        .get(manifest_url)
        .send()
        .await
        .map_err(|e| format!("Manifest request failed: {}", e))?;

    let manifest: serde_json::Value = manifest_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse manifest: {}", e))?;

    Ok(manifest)
}

#[tauri::command]
pub async fn get_extension_readme(
    namespace: String,
    name: String,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let detail_resp = client
        .get(format!("{OPEN_VSX_BASE}/{namespace}/{name}"))
        .send()
        .await
        .map_err(|e| format!("Detail request failed: {}", e))?;

    let detail: ExtensionDetail = detail_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse detail: {}", e))?;

    let readme_url = detail
        .files
        .as_ref()
        .and_then(|f| f.readme.as_ref())
        .ok_or("No README URL available")?;

    let readme_resp = client
        .get(readme_url)
        .send()
        .await
        .map_err(|e| format!("README request failed: {}", e))?;

    let readme = readme_resp
        .text()
        .await
        .map_err(|e| format!("Failed to read README: {}", e))?;

    Ok(readme)
}

#[tauri::command]
pub async fn get_namespace_extensions(namespace: String) -> Result<NamespaceDetail, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{OPEN_VSX_BASE}/{namespace}"))
        .send()
        .await
        .map_err(|e| format!("Namespace request failed: {}", e))?;

    let detail: NamespaceDetail = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse namespace: {}", e))?;

    Ok(detail)
}

#[tauri::command]
pub async fn get_extension_reviews(
    namespace: String,
    name: String,
) -> Result<Vec<Review>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{OPEN_VSX_BASE}/{namespace}/{name}/reviews"))
        .send()
        .await
        .map_err(|e| format!("Reviews request failed: {}", e))?;

    let reviews: Vec<Review> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse reviews: {}", e))?;

    Ok(reviews)
}

#[tauri::command]
pub async fn check_extension_compatibility(
    namespace: String,
    name: String,
) -> Result<CompatibilityReport, String> {
    let manifest = get_extension_manifest(namespace, name).await?;
    Ok(analyze_compatibility(&manifest))
}

#[tauri::command]
pub async fn browse_extensions_by_category(
    category: String,
    offset: Option<u32>,
    size: Option<u32>,
    sort_by: Option<String>,
) -> Result<SearchResult, String> {
    search_extensions(
        String::new(),
        Some(category),
        offset,
        size,
        sort_by,
        None,
    )
    .await
}

// ── Tauri Commands: Extension Management ─────────────────────────────────

#[tauri::command]
pub async fn list_installed_extensions(
    state: tauri::State<'_, ExtensionManager>,
) -> Result<Vec<InstalledExtension>, String> {
    let registry = state.registry.lock().map_err(|e| e.to_string())?;
    Ok(registry.values().cloned().collect())
}

#[tauri::command]
pub async fn enable_extension(
    id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<(), String> {
    let mut registry = state.registry.lock().map_err(|e| e.to_string())?;
    if let Some(ext) = registry.get_mut(&id) {
        ext.enabled = true;
        ExtensionManager::save_registry(&state.extensions_dir, &registry)?;
        Ok(())
    } else {
        Err(format!("Extension '{}' not found", id))
    }
}

#[tauri::command]
pub async fn disable_extension(
    id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<(), String> {
    let mut registry = state.registry.lock().map_err(|e| e.to_string())?;
    if let Some(ext) = registry.get_mut(&id) {
        ext.enabled = false;
        ExtensionManager::save_registry(&state.extensions_dir, &registry)?;
        Ok(())
    } else {
        Err(format!("Extension '{}' not found", id))
    }
}

#[tauri::command]
pub async fn get_extension_package_json(
    id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<serde_json::Value, String> {
    let registry = state.registry.lock().map_err(|e| e.to_string())?;
    let ext = registry
        .get(&id)
        .ok_or(format!("Extension '{}' not found", id))?;
    let pkg_path = PathBuf::from(&ext.path).join("package.json");
    let data = std::fs::read_to_string(&pkg_path)
        .map_err(|e| format!("Failed to read package.json: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse package.json: {}", e))?;
    Ok(json)
}

#[tauri::command]
pub async fn install_extension_from_registry(
    namespace: String,
    name: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<InstalledExtension, String> {
    let ext_id = format!("{}.{}", namespace, name);

    // Emit: starting
    let _ = app_handle.emit(
        "extension-install-progress",
        InstallProgress {
            extension_id: ext_id.clone(),
            stage: "fetching".to_string(),
            percent: 0,
        },
    );

    // 1. Fetch extension details
    let detail = get_extension_details(namespace.clone(), name.clone()).await?;
    let download_url = detail
        .files
        .as_ref()
        .and_then(|f| f.download.as_ref())
        .ok_or("No download URL available")?
        .clone();
    let icon_url = detail
        .files
        .as_ref()
        .and_then(|f| f.icon.as_ref())
        .cloned();

    // Emit: downloading
    let _ = app_handle.emit(
        "extension-install-progress",
        InstallProgress {
            extension_id: ext_id.clone(),
            stage: "downloading".to_string(),
            percent: 10,
        },
    );

    // 2. Download VSIX
    let client = reqwest::Client::new();
    let vsix_bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;

    // Emit: extracting
    let _ = app_handle.emit(
        "extension-install-progress",
        InstallProgress {
            extension_id: ext_id.clone(),
            stage: "extracting".to_string(),
            percent: 50,
        },
    );

    // 3. Extract to extensions dir
    let ext_dir = state.extensions_dir.join(&ext_id);
    if ext_dir.exists() {
        std::fs::remove_dir_all(&ext_dir)
            .map_err(|e| format!("Failed to clean existing extension: {}", e))?;
    }
    std::fs::create_dir_all(&ext_dir)
        .map_err(|e| format!("Failed to create extension dir: {}", e))?;

    // Extract ZIP
    let cursor = std::io::Cursor::new(vsix_bytes.to_vec());
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Failed to open VSIX: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read VSIX entry: {}", e))?;
        let outpath = match file.enclosed_name() {
            Some(path) => {
                // VSIX files have an "extension/" prefix for the actual content
                let path_str = path.to_string_lossy();
                if path_str.starts_with("extension/") {
                    ext_dir.join(path_str.strip_prefix("extension/").unwrap_or(&path_str))
                } else if path_str == "[Content_Types].xml"
                    || path_str.starts_with("extension.vsixmanifest")
                {
                    // Skip VSIX metadata files
                    continue;
                } else {
                    ext_dir.join(&*path_str)
                }
            }
            None => continue,
        };

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut outfile = std::fs::File::create(&outpath)
                .map_err(|e| format!("Failed to create file {}: {}", outpath.display(), e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to write file: {}", e))?;
        }
    }

    // Emit: parsing
    let _ = app_handle.emit(
        "extension-install-progress",
        InstallProgress {
            extension_id: ext_id.clone(),
            stage: "parsing".to_string(),
            percent: 80,
        },
    );

    // 4. Parse package.json
    let pkg_path = ext_dir.join("package.json");
    let pkg_data = std::fs::read_to_string(&pkg_path)
        .map_err(|e| format!("Extension has no package.json: {}", e))?;
    let pkg_json: serde_json::Value =
        serde_json::from_str(&pkg_data).map_err(|e| format!("Invalid package.json: {}", e))?;

    let contributions = parse_contributions(&pkg_json);

    // 5. Download icon if available
    let icon_path = if let Some(url) = icon_url {
        let icon_dest = ext_dir.join("icon.png");
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(bytes) = resp.bytes().await {
                std::fs::write(&icon_dest, &bytes).ok();
                Some(icon_dest.to_string_lossy().to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        // Check if there's an icon in the extracted package
        let pkg_icon = pkg_json
            .get("icon")
            .and_then(|i| i.as_str())
            .map(|p| ext_dir.join(p));
        pkg_icon
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
    };

    // 6. Create InstalledExtension
    let installed = InstalledExtension {
        id: ext_id.clone(),
        display_name: detail
            .display_name
            .unwrap_or_else(|| name.clone()),
        version: detail.version.clone(),
        description: detail.description.unwrap_or_default(),
        enabled: true,
        path: ext_dir.to_string_lossy().to_string(),
        contributions,
        publisher: namespace.clone(),
        icon_path,
    };

    // 7. Update registry
    {
        let mut registry = state.registry.lock().map_err(|e| e.to_string())?;
        registry.insert(ext_id.clone(), installed.clone());
        ExtensionManager::save_registry(&state.extensions_dir, &registry)?;
    }

    // Emit: complete
    let _ = app_handle.emit(
        "extension-install-progress",
        InstallProgress {
            extension_id: ext_id,
            stage: "complete".to_string(),
            percent: 100,
        },
    );

    Ok(installed)
}

#[tauri::command]
pub async fn uninstall_extension(
    id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<(), String> {
    let mut registry = state.registry.lock().map_err(|e| e.to_string())?;
    if let Some(ext) = registry.remove(&id) {
        // Remove files
        let ext_path = PathBuf::from(&ext.path);
        if ext_path.exists() {
            std::fs::remove_dir_all(&ext_path)
                .map_err(|e| format!("Failed to remove extension files: {}", e))?;
        }
        ExtensionManager::save_registry(&state.extensions_dir, &registry)?;
        Ok(())
    } else {
        Err(format!("Extension '{}' not found", id))
    }
}

#[tauri::command]
pub async fn sideload_vsix(
    path: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<InstalledExtension, String> {
    let vsix_path = PathBuf::from(&path);
    if !vsix_path.exists() {
        return Err(format!("File not found: {}", path));
    }

    // Read VSIX file
    let vsix_data =
        std::fs::read(&vsix_path).map_err(|e| format!("Failed to read VSIX: {}", e))?;
    let cursor = std::io::Cursor::new(vsix_data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Failed to open VSIX: {}", e))?;

    // Find and parse package.json to get the extension ID
    let pkg_json: serde_json::Value = {
        let mut pkg_file = None;
        for i in 0..archive.len() {
            let file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read entry: {}", e))?;
            let name = file.name().to_string();
            if name == "extension/package.json" || name == "package.json" {
                pkg_file = Some(i);
                break;
            }
        }
        let idx = pkg_file.ok_or("No package.json found in VSIX")?;
        let file = archive.by_index(idx).map_err(|e| e.to_string())?;
        serde_json::from_reader(file).map_err(|e| format!("Invalid package.json: {}", e))?
    };

    let publisher = pkg_json
        .get("publisher")
        .and_then(|p| p.as_str())
        .ok_or("package.json missing publisher field")?;
    let name = pkg_json
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or("package.json missing name field")?;
    let ext_id = format!("{}.{}", publisher, name);

    // Extract to extension dir
    let ext_dir = state.extensions_dir.join(&ext_id);
    if ext_dir.exists() {
        std::fs::remove_dir_all(&ext_dir).ok();
    }
    std::fs::create_dir_all(&ext_dir)
        .map_err(|e| format!("Failed to create dir: {}", e))?;

    // Re-read since we consumed the archive
    let vsix_data =
        std::fs::read(&vsix_path).map_err(|e| format!("Failed to re-read VSIX: {}", e))?;
    let cursor = std::io::Cursor::new(vsix_data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => {
                let path_str = path.to_string_lossy();
                if path_str.starts_with("extension/") {
                    ext_dir.join(path_str.strip_prefix("extension/").unwrap_or(&path_str))
                } else if path_str == "[Content_Types].xml"
                    || path_str.starts_with("extension.vsixmanifest")
                {
                    continue;
                } else {
                    ext_dir.join(&*path_str)
                }
            }
            None => continue,
        };

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    let contributions = parse_contributions(&pkg_json);

    let version = pkg_json
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let display_name = pkg_json
        .get("displayName")
        .and_then(|d| d.as_str())
        .unwrap_or(name)
        .to_string();
    let description = pkg_json
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();

    let icon_path = pkg_json
        .get("icon")
        .and_then(|i| i.as_str())
        .map(|p| ext_dir.join(p))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string());

    let installed = InstalledExtension {
        id: ext_id.clone(),
        display_name,
        version,
        description,
        enabled: true,
        path: ext_dir.to_string_lossy().to_string(),
        contributions,
        publisher: publisher.to_string(),
        icon_path,
    };

    // Update registry
    {
        let mut registry = state.registry.lock().map_err(|e| e.to_string())?;
        registry.insert(ext_id.clone(), installed.clone());
        ExtensionManager::save_registry(&state.extensions_dir, &registry)?;
    }

    let _ = app_handle.emit(
        "extension-install-progress",
        InstallProgress {
            extension_id: ext_id,
            stage: "complete".to_string(),
            percent: 100,
        },
    );

    Ok(installed)
}

/// Read the content of a theme file from an installed extension
#[tauri::command]
pub async fn read_extension_theme(
    extension_id: String,
    theme_path: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<serde_json::Value, String> {
    let registry = state.registry.lock().map_err(|e| e.to_string())?;
    let ext = registry
        .get(&extension_id)
        .ok_or(format!("Extension '{}' not found", extension_id))?;
    let full_path = PathBuf::from(&ext.path).join(&theme_path);
    let data = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read theme file: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse theme JSON: {}", e))?;
    Ok(json)
}

/// Read snippet file content from an installed extension
#[tauri::command]
pub async fn read_extension_snippets(
    extension_id: String,
    snippet_path: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<serde_json::Value, String> {
    let registry = state.registry.lock().map_err(|e| e.to_string())?;
    let ext = registry
        .get(&extension_id)
        .ok_or(format!("Extension '{}' not found", extension_id))?;
    let full_path = PathBuf::from(&ext.path).join(&snippet_path);
    let data = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read snippets file: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse snippets: {}", e))?;
    Ok(json)
}

// ── LSP Server Management ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerInfo {
    pub server_id: String,
    pub extension_id: String,
    pub languages: Vec<String>,
}

/// Start a language server process for an installed extension.
/// Returns a server_id used to route messages.
#[tauri::command]
pub async fn start_language_server(
    extension_id: String,
    server_command: String,
    server_args: Vec<String>,
    workspace_path: String,
    languages: Vec<String>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<LspServerInfo, String> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Spawn the language server process with stdin/stdout pipes
    let mut child = Command::new(&server_command)
        .args(&server_args)
        .current_dir(&workspace_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start language server '{}': {}", server_command, e))?;

    // Take stdout for reading LSP responses
    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture language server stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture language server stderr")?;

    let handle = LanguageServerHandle {
        extension_id: extension_id.clone(),
        server_id: server_id.clone(),
        child: Mutex::new(child),
        languages: languages.clone(),
    };

    // Store the handle
    {
        let mut servers = state.running_servers.lock().map_err(|e| e.to_string())?;
        servers.insert(server_id.clone(), handle);
    }

    // Background thread to read LSP stdout and emit events
    let sid = server_id.clone();
    let app = app_handle.clone();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut reader = std::io::BufReader::new(stdout);
        let mut header_buf = String::new();

        loop {
            header_buf.clear();
            // Read LSP headers (Content-Length: NNN\r\n\r\n)
            loop {
                let mut byte = [0u8; 1];
                match reader.read_exact(&mut byte) {
                    Ok(_) => header_buf.push(byte[0] as char),
                    Err(_) => return, // Server exited
                }
                if header_buf.ends_with("\r\n\r\n") {
                    break;
                }
            }

            // Parse Content-Length
            let content_length: usize = header_buf
                .lines()
                .find_map(|line| {
                    if line.to_lowercase().starts_with("content-length:") {
                        line.split(':').nth(1)?.trim().parse().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            if content_length == 0 {
                continue;
            }

            // Read the JSON body
            let mut body = vec![0u8; content_length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }

            if let Ok(body_str) = String::from_utf8(body) {
                let _ = app.emit(&format!("lsp-message-{}", sid), &body_str);
            }
        }
    });

    // Background thread to read stderr (for debugging)
    let sid2 = server_id.clone();
    let app2 = app_handle.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                let _ = app2.emit(&format!("lsp-stderr-{}", sid2), &line);
            }
        }
    });

    let info = LspServerInfo {
        server_id,
        extension_id,
        languages,
    };

    Ok(info)
}

/// Send a JSON-RPC message to a running language server via its stdin.
#[tauri::command]
pub async fn send_lsp_message(
    server_id: String,
    message: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<(), String> {
    let servers = state.running_servers.lock().map_err(|e| e.to_string())?;
    let handle = servers
        .get(&server_id)
        .ok_or(format!("Language server '{}' not found", server_id))?;

    let mut child = handle.child.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut stdin) = child.stdin {
        use std::io::Write;
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        stdin
            .write_all(header.as_bytes())
            .map_err(|e| format!("Failed to write LSP header: {}", e))?;
        stdin
            .write_all(message.as_bytes())
            .map_err(|e| format!("Failed to write LSP message: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("Failed to flush LSP stdin: {}", e))?;
        Ok(())
    } else {
        Err("Language server stdin not available".to_string())
    }
}

/// Stop a running language server.
#[tauri::command]
pub async fn stop_language_server(
    server_id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<(), String> {
    let mut servers = state.running_servers.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = servers.remove(&server_id) {
        let mut child = handle.child.lock().map_err(|e| e.to_string())?;
        let _ = child.kill();
        Ok(())
    } else {
        Err(format!("Language server '{}' not found", server_id))
    }
}

/// List all running language servers.
#[tauri::command]
pub async fn list_language_servers(
    state: tauri::State<'_, ExtensionManager>,
) -> Result<Vec<LspServerInfo>, String> {
    let servers = state.running_servers.lock().map_err(|e| e.to_string())?;
    Ok(servers
        .values()
        .map(|h| LspServerInfo {
            server_id: h.server_id.clone(),
            extension_id: h.extension_id.clone(),
            languages: h.languages.clone(),
        })
        .collect())
}
