use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::Emitter;

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
        let extensions_dir = crate::platform::config_dir().join("extensions");
        std::fs::create_dir_all(&extensions_dir).ok();

        let registry = Self::load_registry(&extensions_dir);
        Self {
            registry: Mutex::new(registry),
            extensions_dir,
            running_servers: Mutex::new(HashMap::new()),
        }
    }

    fn registry_path(extensions_dir: &std::path::Path) -> PathBuf {
        extensions_dir.join("registry.json")
    }

    fn load_registry(extensions_dir: &std::path::Path) -> HashMap<String, InstalledExtension> {
        let path = Self::registry_path(extensions_dir);
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn save_registry(
        extensions_dir: &std::path::Path,
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
pub async fn get_extension_readme(namespace: String, name: String) -> Result<String, String> {
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
pub async fn get_extension_reviews(namespace: String, name: String) -> Result<Vec<Review>, String> {
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
    search_extensions(String::new(), Some(category), offset, size, sort_by, None).await
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
    let icon_url = detail.files.as_ref().and_then(|f| f.icon.as_ref()).cloned();

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
        display_name: detail.display_name.unwrap_or_else(|| name.clone()),
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
    let vsix_data = std::fs::read(&vsix_path).map_err(|e| format!("Failed to read VSIX: {}", e))?;
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
    std::fs::create_dir_all(&ext_dir).map_err(|e| format!("Failed to create dir: {}", e))?;

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
        .map_err(|e| {
            format!(
                "Failed to start language server '{}': {}",
                server_command, e
            )
        })?;

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
                    Err(_) => {
                        // Server exited — emit exit event for crash detection
                        let _ = app.emit(&format!("lsp-server-exit-{}", sid), "crashed");
                        return;
                    }
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
                let _ = app.emit(&format!("lsp-server-exit-{}", sid), "crashed");
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
        for line in reader.lines().map_while(Result::ok) {
            let _ = app2.emit(&format!("lsp-stderr-{}", sid2), &line);
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

/// Get configuration schema for an installed extension.
/// Parses contributes.configuration from the extension's package.json.
#[tauri::command]
pub async fn get_extension_config_schema(
    id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<serde_json::Value, String> {
    let registry = state.registry.lock().map_err(|e| e.to_string())?;
    let ext = registry
        .values()
        .find(|e| e.id == id)
        .ok_or(format!("Extension '{}' not found", id))?;

    Ok(ext
        .contributions
        .configuration
        .clone()
        .unwrap_or(serde_json::Value::Null))
}

/// Get settings for a specific extension.
#[tauri::command]
pub async fn get_extension_settings(
    id: String,
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
) -> Result<serde_json::Value, String> {
    let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings
        .extension_settings
        .get(&id)
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())))
}

/// Update settings for a specific extension.
#[tauri::command]
pub async fn update_extension_settings(
    id: String,
    values: serde_json::Value,
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
) -> Result<(), String> {
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
    settings.extension_settings.insert(id, values);
    // Save to disk
    drop(settings);
    // Re-acquire to save
    let s = settings_state.settings.lock().map_err(|e| e.to_string())?;
    let path =
        super::settings::SettingsManager::config_path().ok_or("Cannot determine config path")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&*s).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Phase 9: Polish & Reliability ────────────────────────────────────────

/// Phase 9.1 — Extension update checking
/// Fetches update information for all installed extensions from Open VSX.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAvailable {
    pub extension_id: String,
    pub current_version: String,
    pub latest_version: String,
    pub display_name: String,
}

#[tauri::command]
pub async fn check_extension_updates(
    state: tauri::State<'_, ExtensionManager>,
) -> Result<Vec<UpdateAvailable>, String> {
    let installed: Vec<_> = {
        let registry = state.registry.lock().map_err(|e| e.to_string())?;
        registry.values().cloned().collect()
    };

    let client = reqwest::Client::new();
    let mut updates = Vec::new();

    for ext in &installed {
        // ext.id is "namespace.name"
        let parts: Vec<&str> = ext.id.splitn(2, '.').collect();
        if parts.len() != 2 {
            continue;
        }
        let (namespace, name) = (parts[0], parts[1]);

        let url = format!("{}/{}/{}", "https://open-vsx.org/api", namespace, name);
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(detail) = resp.json::<serde_json::Value>().await {
                if let Some(latest) = detail["version"].as_str() {
                    if latest != ext.version {
                        updates.push(UpdateAvailable {
                            extension_id: ext.id.clone(),
                            current_version: ext.version.clone(),
                            latest_version: latest.to_string(),
                            display_name: ext.display_name.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(updates)
}

/// Phase 9.2 — Extension recommendations
/// Returns recommended extensions for a given language based on curated mappings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionRecommendation {
    pub language_id: String,
    pub namespace: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
}

#[tauri::command]
pub async fn get_extension_recommendations(
    language_id: String,
    state: tauri::State<'_, ExtensionManager>,
) -> Result<Vec<ExtensionRecommendation>, String> {
    // Curated mapping of language to recommended extensions
    let recommendations: Vec<(&str, &str, &str, &str, &str)> = vec![
        (
            "python",
            "ms-python",
            "python",
            "Python",
            "Python language support with IntelliSense",
        ),
        (
            "rust",
            "rust-lang",
            "rust-analyzer",
            "rust-analyzer",
            "Rust language support",
        ),
        ("go", "golang", "go", "Go", "Go language support"),
        (
            "yaml",
            "redhat",
            "vscode-yaml",
            "YAML",
            "YAML language support",
        ),
        ("r", "REditorSupport", "r", "R", "R language support"),
        ("java", "redhat", "java", "Java", "Java language support"),
        (
            "typescript",
            "denoland",
            "vscode-deno",
            "Deno",
            "Deno/TypeScript support",
        ),
        (
            "typescriptreact",
            "denoland",
            "vscode-deno",
            "Deno",
            "Deno/TypeScript React support",
        ),
        (
            "json",
            "vscode",
            "json-language-features",
            "JSON",
            "JSON language features",
        ),
        (
            "html",
            "nicolo-ribaudo",
            "vscode-html",
            "HTML",
            "HTML language support",
        ),
        (
            "css",
            "nicolo-ribaudo",
            "vscode-css",
            "CSS",
            "CSS language support",
        ),
        (
            "toml",
            "tamasfe",
            "even-better-toml",
            "Even Better TOML",
            "TOML language support",
        ),
        (
            "markdown",
            "yzhang",
            "markdown-all-in-one",
            "Markdown All in One",
            "Markdown tools",
        ),
        (
            "sql",
            "mtxr",
            "sqltools",
            "SQLTools",
            "SQL development tools",
        ),
        (
            "dockerfile",
            "exiasr",
            "hadolint",
            "hadolint",
            "Dockerfile linting",
        ),
        (
            "shell",
            "timonwong",
            "shellcheck",
            "ShellCheck",
            "Shell script analysis",
        ),
    ];

    // Filter: only return recommendations for the requested language
    // AND that are not already installed
    let registry = state.registry.lock().map_err(|e| e.to_string())?;
    let installed_ids: std::collections::HashSet<String> =
        registry.values().map(|e| e.id.clone()).collect();
    drop(registry);

    let results: Vec<ExtensionRecommendation> = recommendations
        .iter()
        .filter(|(lang, ns, name, _, _)| {
            *lang == language_id && !installed_ids.contains(&format!("{}.{}", ns, name))
        })
        .map(|(lang, ns, name, display, desc)| ExtensionRecommendation {
            language_id: lang.to_string(),
            namespace: ns.to_string(),
            name: name.to_string(),
            display_name: display.to_string(),
            description: desc.to_string(),
        })
        .collect();

    Ok(results)
}

/// Phase 9.3 — Compatibility validation
/// Validates if an extension can be installed, checking engine compatibility,
/// extension kind, and platform support.
#[tauri::command]
pub async fn validate_extension_install(
    namespace: String,
    name: String,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}/{}", "https://open-vsx.org/api", namespace, name);
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let detail: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let mut warnings: Vec<String> = Vec::new();
    let mut can_install = true;

    // Check engine compatibility
    if let Some(engines) = detail.get("engines") {
        if let Some(vscode_ver) = engines.get("vscode").and_then(|v| v.as_str()) {
            // We support up to ~1.85 level API
            if vscode_ver.contains("1.9") || vscode_ver.contains("2.") {
                warnings.push(format!(
                    "Requires VS Code {}, which may be newer than Operon supports",
                    vscode_ver
                ));
            }
        }
    }

    // Check for unsupported extension kinds
    if let Some(kinds) = detail.get("extensionKind").and_then(|k| k.as_array()) {
        let kind_strs: Vec<&str> = kinds.iter().filter_map(|k| k.as_str()).collect();
        if kind_strs.contains(&"ui") && !kind_strs.contains(&"workspace") {
            warnings.push("This extension is UI-only and may not work in Operon".to_string());
            can_install = false;
        }
    }

    // Check platform
    if let Some(target) = detail.get("targetPlatform").and_then(|t| t.as_str()) {
        if target != "universal" && target != "web" {
            let current_platform = if cfg!(target_os = "macos") {
                "darwin"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else {
                "unknown"
            };
            if !target.contains(current_platform) {
                warnings.push(format!(
                    "Extension targets {}, but you're on {}",
                    target, current_platform
                ));
            }
        }
    }

    Ok(serde_json::json!({
        "can_install": can_install,
        "warnings": warnings,
    }))
}

// ── Docker CLI Integration ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainer {
    pub id: String,
    pub names: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub ports: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
}

#[tauri::command]
pub async fn docker_list_containers() -> Result<Vec<DockerContainer>, String> {
    let output = std::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .output()
        .map_err(|e| format!("Docker not available: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            containers.push(DockerContainer {
                id: val["ID"].as_str().unwrap_or("").to_string(),
                names: val["Names"].as_str().unwrap_or("").to_string(),
                image: val["Image"].as_str().unwrap_or("").to_string(),
                status: val["Status"].as_str().unwrap_or("").to_string(),
                state: val["State"].as_str().unwrap_or("").to_string(),
                ports: val["Ports"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(containers)
}

#[tauri::command]
pub async fn docker_list_images() -> Result<Vec<DockerImage>, String> {
    let output = std::process::Command::new("docker")
        .args(["images", "--format", "{{json .}}"])
        .output()
        .map_err(|e| format!("Docker not available: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut images = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            images.push(DockerImage {
                id: val["ID"].as_str().unwrap_or("").to_string(),
                repository: val["Repository"].as_str().unwrap_or("").to_string(),
                tag: val["Tag"].as_str().unwrap_or("").to_string(),
                size: val["Size"].as_str().unwrap_or("").to_string(),
                created: val["CreatedSince"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(images)
}

#[tauri::command]
pub async fn docker_list_volumes() -> Result<Vec<DockerVolume>, String> {
    let output = std::process::Command::new("docker")
        .args(["volume", "ls", "--format", "{{json .}}"])
        .output()
        .map_err(|e| format!("Docker not available: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut volumes = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            volumes.push(DockerVolume {
                name: val["Name"].as_str().unwrap_or("").to_string(),
                driver: val["Driver"].as_str().unwrap_or("").to_string(),
                mountpoint: val["Mountpoint"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(volumes)
}

#[tauri::command]
pub async fn docker_container_action(
    container_id: String,
    action: String,
) -> Result<String, String> {
    let args = match action.as_str() {
        "start" => vec!["start", &container_id],
        "stop" => vec!["stop", &container_id],
        "remove" => vec!["rm", "-f", &container_id],
        "logs" => vec!["logs", "--tail", "100", &container_id],
        "restart" => vec!["restart", &container_id],
        _ => return Err(format!("Unknown action: {}", action)),
    };

    let output = std::process::Command::new("docker")
        .args(&args)
        .output()
        .map_err(|e| format!("Docker command failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ── Singularity / Apptainer CLI Integration ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingularityImage {
    pub name: String,
    pub path: String,
    pub size: String,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingularityInstance {
    pub name: String,
    pub pid: String,
    pub image: String,
}

#[tauri::command]
pub async fn singularity_list_images(search_dir: String) -> Result<Vec<SingularityImage>, String> {
    // Find .sif files in the search directory
    let output = std::process::Command::new("find")
        .args([
            &search_dir,
            "-maxdepth",
            "3",
            "-name",
            "*.sif",
            "-type",
            "f",
        ])
        .output()
        .map_err(|e| format!("Failed to search for images: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut images = Vec::new();
    for line in stdout.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let meta = std::fs::metadata(path);
        let (size, modified) = match meta {
            Ok(m) => {
                let size_mb = m.len() as f64 / 1_048_576.0;
                let size_str = if size_mb > 1024.0 {
                    format!("{:.1} GB", size_mb / 1024.0)
                } else {
                    format!("{:.0} MB", size_mb)
                };
                let mod_time = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        let secs = d.as_secs();
                        let days_ago = (std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                            - secs)
                            / 86400;
                        if days_ago == 0 {
                            "today".to_string()
                        } else if days_ago == 1 {
                            "yesterday".to_string()
                        } else {
                            format!("{} days ago", days_ago)
                        }
                    })
                    .unwrap_or_default();
                (size_str, mod_time)
            }
            Err(_) => ("unknown".to_string(), "unknown".to_string()),
        };
        images.push(SingularityImage {
            name,
            path: path.to_string(),
            size,
            modified,
        });
    }
    Ok(images)
}

#[tauri::command]
pub async fn singularity_list_instances() -> Result<Vec<SingularityInstance>, String> {
    // Try apptainer first, then singularity
    let cmd = if std::process::Command::new("apptainer")
        .arg("--version")
        .output()
        .is_ok()
    {
        "apptainer"
    } else {
        "singularity"
    };

    let output = std::process::Command::new(cmd)
        .args(["instance", "list", "--json"])
        .output()
        .map_err(|e| format!("{} not available: {}", cmd, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut instances = Vec::new();

    // Parse JSON output
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(list) = val["instances"].as_array() {
            for inst in list {
                instances.push(SingularityInstance {
                    name: inst["instance"].as_str().unwrap_or("").to_string(),
                    pid: inst["pid"]
                        .as_u64()
                        .map(|p| p.to_string())
                        .unwrap_or_default(),
                    image: inst["img"].as_str().unwrap_or("").to_string(),
                });
            }
        }
    }
    Ok(instances)
}

#[tauri::command]
pub async fn singularity_action(
    action: String,
    image_path: String,
    instance_name: Option<String>,
) -> Result<String, String> {
    let cmd = if std::process::Command::new("apptainer")
        .arg("--version")
        .output()
        .is_ok()
    {
        "apptainer"
    } else {
        "singularity"
    };

    let mut args: Vec<String> = Vec::new();
    match action.as_str() {
        "shell" | "exec" | "run" => {
            args.push(action.clone());
            args.push(image_path);
        }
        "instance_start" => {
            args.push("instance".to_string());
            args.push("start".to_string());
            args.push(image_path);
            args.push(instance_name.unwrap_or_else(|| "default".to_string()));
        }
        "instance_stop" => {
            args.push("instance".to_string());
            args.push("stop".to_string());
            args.push(instance_name.unwrap_or_else(|| "default".to_string()));
        }
        "pull" => {
            args.push("pull".to_string());
            args.push(image_path); // This is the URI for pull
        }
        _ => return Err(format!("Unknown singularity action: {}", action)),
    }

    let output = std::process::Command::new(cmd)
        .args(args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .output()
        .map_err(|e| format!("{} command failed: {}", cmd, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            return Err(stderr.to_string());
        }
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Start a language server on a remote machine via SSH.
/// The LSP server runs on the remote and messages are relayed through SSH.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_remote_language_server(
    extension_id: String,
    server_command: String,
    server_args: Vec<String>,
    workspace_path: String,
    languages: Vec<String>,
    ssh_profile_id: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ExtensionManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
) -> Result<LspServerInfo, String> {
    // Build the remote command: start the language server via SSH
    let args_str = server_args.join(" ");
    let remote_cmd = format!(
        "cd {} && {} {} 2>/dev/null",
        workspace_path, server_command, args_str
    );

    // Get SSH profile
    let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p.id == ssh_profile_id)
        .ok_or(format!("SSH profile '{}' not found", ssh_profile_id))?
        .clone();
    drop(profiles);

    let server_id = uuid::Uuid::new_v4().to_string();

    // Spawn SSH process with the language server command
    // The SSH session acts as a pipe: stdin → remote server stdin, remote server stdout → our stdout
    let mut ssh_args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
    ];

    // Add port if non-default
    if profile.port != 22 {
        ssh_args.push("-p".to_string());
        ssh_args.push(profile.port.to_string());
    }

    // Add identity file if specified
    if let Some(ref key) = profile.key_file {
        if !key.is_empty() {
            ssh_args.push("-i".to_string());
            ssh_args.push(key.clone());
        }
    }

    let host_str = format!("{}@{}", profile.user, profile.host);
    ssh_args.push(host_str);
    ssh_args.push(remote_cmd);

    let mut child = std::process::Command::new("ssh")
        .args(&ssh_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start remote language server via SSH: {}", e))?;

    let stdout = child.stdout.take().ok_or("Failed to capture SSH stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture SSH stderr")?;

    let handle = LanguageServerHandle {
        extension_id: extension_id.clone(),
        server_id: server_id.clone(),
        child: Mutex::new(child),
        languages: languages.clone(),
    };

    {
        let mut servers = state.running_servers.lock().map_err(|e| e.to_string())?;
        servers.insert(server_id.clone(), handle);
    }

    // Stdout relay thread (same as local — reads LSP Content-Length framed messages)
    let sid = server_id.clone();
    let app = app_handle.clone();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut reader = std::io::BufReader::new(stdout);
        let mut header_buf = String::new();
        loop {
            header_buf.clear();
            loop {
                let mut byte = [0u8; 1];
                match reader.read_exact(&mut byte) {
                    Ok(_) => header_buf.push(byte[0] as char),
                    Err(_) => return,
                }
                if header_buf.ends_with("\r\n\r\n") {
                    break;
                }
            }
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
            let mut body = vec![0u8; content_length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            if let Ok(body_str) = String::from_utf8(body) {
                let _ = app.emit(&format!("lsp-message-{}", sid), &body_str);
            }
        }
    });

    // Stderr relay thread
    let sid2 = server_id.clone();
    let app2 = app_handle;
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = app2.emit(&format!("lsp-stderr-{}", sid2), &line);
        }
    });

    Ok(LspServerInfo {
        server_id,
        extension_id,
        languages,
    })
}

/// Install an extension on a remote machine.
/// Downloads the VSIX locally, copies to remote, and extracts.
#[tauri::command]
pub async fn install_remote_extension(
    ssh_profile_id: String,
    namespace: String,
    name: String,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
) -> Result<(), String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == ssh_profile_id)
            .ok_or(format!("SSH profile '{}' not found", ssh_profile_id))?
            .clone()
    };

    // Download extension metadata to get VSIX URL
    let url = format!("{}/{}/{}", "https://open-vsx.org/api", namespace, name);
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let detail: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let download_url = detail["files"]["download"]
        .as_str()
        .ok_or("No download URL found")?
        .to_string();

    // Download VSIX to temp
    let tmp_path = crate::platform::temp_dir().join(format!("{}.{}.vsix", namespace, name));
    let resp = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(&tmp_path, &bytes).map_err(|e| e.to_string())?;

    // SCP to remote
    let remote_dir = "~/.operon/extensions";
    let ext_id = format!("{}.{}", namespace, name);
    let remote_ext_dir = format!("{}/{}", remote_dir, ext_id);

    // Create remote directory and copy
    let mkdir_cmd = format!("mkdir -p {}", remote_ext_dir);
    super::ssh::ssh_exec(&profile, &mkdir_cmd)
        .map_err(|e| format!("Failed to create remote dir: {}", e))?;

    // SCP the VSIX file
    let mut scp_args = vec!["-o".to_string(), "StrictHostKeyChecking=no".to_string()];
    if profile.port != 22 {
        scp_args.push("-P".to_string());
        scp_args.push(profile.port.to_string());
    }
    if let Some(ref key) = profile.key_file {
        if !key.is_empty() {
            scp_args.push("-i".to_string());
            scp_args.push(key.clone());
        }
    }
    let host_str = format!("{}@{}", profile.user, profile.host);
    scp_args.push(tmp_path.to_string_lossy().to_string());
    scp_args.push(format!("{}:{}/{}.vsix", host_str, remote_ext_dir, ext_id));

    std::process::Command::new("scp")
        .args(&scp_args)
        .output()
        .map_err(|e| format!("SCP failed: {}", e))?;

    // Extract on remote
    let extract_cmd = format!(
        "cd {} && unzip -o {}.vsix -d . 2>/dev/null || python3 -c \"import zipfile; zipfile.ZipFile('{}.vsix').extractall('.')\" 2>/dev/null; rm -f {}.vsix",
        remote_ext_dir, ext_id, ext_id, ext_id
    );
    super::ssh::ssh_exec(&profile, &extract_cmd)
        .map_err(|e| format!("Remote extraction failed: {}", e))?;

    // Clean up local temp
    let _ = std::fs::remove_file(&tmp_path);

    Ok(())
}
