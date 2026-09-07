use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Data Structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConfig {
    pub name: String,
    pub enabled: bool,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub catalog_id: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tools_count: u32,
    pub tools_summary: Vec<String>,
    pub databases: Vec<String>,
    pub runtime: String,
    pub install_command: String,
    pub config: MCPServerConfig,
    pub homepage: String,
    pub license: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyStatus {
    pub satisfied: bool,
    pub runtime: String,
    pub runtime_found: bool,
    pub runtime_version: Option<String>,
    pub min_version: String,
    pub install_hint: String,
    pub package_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerStatus {
    pub config: MCPServerConfig,
    pub from_catalog: bool,
    pub catalog_entry: Option<MCPCatalogEntry>,
}

// ─── Catalog ─────────────────────────────────────────────────────────────────

pub fn get_research_catalog() -> Vec<MCPCatalogEntry> {
    vec![
        MCPCatalogEntry {
            id: "bio-mcp".into(),
            name: "BioMCP".into(),
            description: "Protein structure analysis — analyze active sites, search disease-related proteins via the Protein Data Bank (PDB)".into(),
            category: "Protein Structure".into(),
            tools_count: 2,
            tools_summary: vec![
                "analyze-active-site: Protein binding site analysis from PDB ID".into(),
                "search-disease-proteins: Find proteins related to diseases".into(),
            ],
            databases: vec!["RCSB PDB".into()],
            runtime: "node".into(),
            install_command: "npm install -g @anthropic-ai/bio-mcp".into(),
            config: MCPServerConfig {
                name: "bio-mcp".into(),
                enabled: false,
                command: "npx".into(),
                args: vec!["@anthropic-ai/bio-mcp".into()],
                env: HashMap::new(),
                catalog_id: Some("bio-mcp".into()),
                description: Some("Protein structure analysis via PDB".into()),
            },
            homepage: "https://github.com/acashmoney/bio-mcp".into(),
            license: "Open Source".into(),
        },
        MCPCatalogEntry {
            id: "encode-toolkit".into(),
            name: "ENCODE Toolkit".into(),
            description: "Comprehensive ENCODE portal access — search experiments, download files, track datasets, compare experiments, export citations across 14 genomic databases".into(),
            category: "Genomics".into(),
            tools_count: 20,
            tools_summary: vec![
                "encode_search_experiments: Search with 20+ filters (assay, organism, biosample, etc.)".into(),
                "encode_get_experiment: Full experiment details with files and quality metrics".into(),
                "encode_download_files: Download with MD5 verification".into(),
                "encode_batch_download: Combined search-and-download with dry-run preview".into(),
                "encode_track_experiment: Local experiment tracking with publication data".into(),
                "encode_list_files: Filter by format, output type, assembly".into(),
                "encode_search_files: Cross-experiment file search".into(),
                "encode_get_metadata: List valid filter values".into(),
                "encode_get_facets: Live counts for filter combinations".into(),
                "encode_get_file_info: Single file metadata".into(),
                "encode_manage_credentials: Authentication for restricted datasets".into(),
                "encode_list_tracked: List locally tracked experiments".into(),
                "encode_get_citations: Export citations".into(),
                "encode_compare_experiments: Comparative analysis".into(),
                "encode_summarize_collection: Summarize experiment collections".into(),
                "encode_log_derived_file: Log derived file operations".into(),
                "encode_get_provenance: Data provenance tracking".into(),
                "encode_export_data: Export analysis results".into(),
                "encode_link_reference: Link external references".into(),
                "encode_get_references: Retrieve reference information".into(),
            ],
            databases: vec![
                "ENCODE".into(), "GTEx".into(), "ClinVar".into(),
                "GWAS Catalog".into(), "JASPAR".into(), "CellxGene".into(),
                "gnomAD".into(), "Ensembl".into(), "UCSC Genome Browser".into(),
                "GEO".into(), "PubMed".into(), "bioRxiv".into(),
                "ClinicalTrials.gov".into(), "Open Targets".into(),
            ],
            runtime: "python".into(),
            install_command: "pip install encode-toolkit".into(),
            config: MCPServerConfig {
                name: "encode".into(),
                enabled: false,
                command: "uvx".into(),
                args: vec!["encode-toolkit".into()],
                env: HashMap::new(),
                catalog_id: Some("encode-toolkit".into()),
                description: Some("ENCODE genomic data access (20 tools across 14 databases)".into()),
            },
            homepage: "https://github.com/ammawla/encode-toolkit".into(),
            license: "Non-Commercial (free for academic/research)".into(),
        },
        MCPCatalogEntry {
            id: "alphagenome".into(),
            name: "AlphaGenome".into(),
            description: "DeepMind AlphaGenome API — predict gene expression, splicing, chromatin features, and variant effects from DNA sequences at single base-pair resolution".into(),
            category: "Variant Analysis".into(),
            tools_count: 8,
            tools_summary: vec![
                "alphagenome_predict_sequence: Generate multimodal predictions for a DNA sequence (up to 1 MB)".into(),
                "alphagenome_predict_interval: Predict genomic outputs for a chromosomal region".into(),
                "alphagenome_predict_variant: Predict functional effects of genetic variants (ref vs alt)".into(),
                "alphagenome_score_variant: Quantify variant effects using multiple scoring methods".into(),
                "alphagenome_validate_sequence: Validate DNA sequence formatting".into(),
                "alphagenome_get_metadata: Retrieve organism model information".into(),
                "alphagenome_get_supported_outputs: List available output types (expression, splicing, etc.)".into(),
                "alphagenome_get_supported_organisms: List supported organisms".into(),
            ],
            databases: vec!["AlphaGenome API".into()],
            runtime: "python".into(),
            install_command: "pip install alphagenome-mcp".into(),
            config: MCPServerConfig {
                name: "alphagenome".into(),
                enabled: false,
                command: "uvx".into(),
                args: vec!["alphagenome-mcp".into(), "stdio".into()],
                env: HashMap::from([
                    ("ALPHA_GENOME_API_KEY".into(), "".into()),
                ]),
                catalog_id: Some("alphagenome".into()),
                description: Some("DeepMind AlphaGenome — variant effects & genomic predictions".into()),
            },
            homepage: "https://github.com/longevity-genie/alphagenome-mcp".into(),
            license: "Open Source".into(),
        },
    ]
}

// ─── Sync MCP servers into Claude Code's native config ──────────────────────

/// Build a `shell_exec` command that runs Claude Code's CLI with the resolved
/// absolute path (Unix) and an augmented PATH. Without this, MCP sync runs a
/// bare `claude mcp …` whose login shell can't see `~/.local/bin` on a
/// Finder/Dock launch — the same "command not found: claude" failure as the
/// chat path. `claude_invocation()` is the bare name on Windows by design (Git
/// Bash resolves it), and the augmented PATH also surfaces an npm-shim `node`.
fn claude_cli(args: &str) -> std::process::Command {
    let mut cmd = crate::platform::shell_exec(&format!(
        "{} {}",
        crate::platform::claude_invocation(),
        args
    ));
    cmd.env("PATH", crate::platform::augmented_path());
    cmd
}

/// Register a single MCP server with Claude Code using `claude mcp add-json`.
/// This makes the server visible to the Claude agent natively (no --mcp-config needed).
/// If the server already exists, it is removed first so env/config updates take effect.
fn claude_mcp_add(server: &MCPServerConfig) -> Result<(), String> {
    use crate::platform::common::shell_escape;
    // Always remove first to handle "already exists" and to pick up env var
    // changes. Routed through the platform shell so Windows finds `claude`
    // even when it is an npm shim (`claude.cmd`) that `Command::new` can
    // neither resolve via PATHEXT nor execute directly.
    let _ = claude_cli(&format!(
        "mcp remove {} -s user",
        shell_escape(&server.name)
    ))
    .output(); // ignore errors — server may not exist yet

    let mut config_obj = serde_json::Map::new();
    config_obj.insert("command".into(), serde_json::json!(server.command));
    config_obj.insert("args".into(), serde_json::json!(server.args));
    if !server.env.is_empty() {
        config_obj.insert("env".into(), serde_json::json!(server.env));
    }
    let json_str = serde_json::to_string(&serde_json::Value::Object(config_obj))
        .map_err(|e| format!("Failed to serialize MCP config: {}", e))?;

    let output = claude_cli(&format!(
        "mcp add-json {} {} -s user",
        shell_escape(&server.name),
        shell_escape(&json_str)
    ))
    .output()
    .map_err(|e| format!("Failed to run `claude mcp add-json`: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "[operon] claude mcp add-json for '{}' failed: {}",
            server.name, stderr
        );
    }
    Ok(())
}

/// Remove an MCP server from Claude Code using `claude mcp remove`.
fn claude_mcp_remove(name: &str) -> Result<(), String> {
    // Via the platform shell + resolved absolute claude — see `claude_cli`.
    let output = claude_cli(&format!(
        "mcp remove {} -s user",
        crate::platform::common::shell_escape(name)
    ))
    .output()
    .map_err(|e| format!("Failed to run `claude mcp remove`: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "[operon] claude mcp remove for '{}' failed: {}",
            name, stderr
        );
    }
    Ok(())
}

/// Sync all Operon MCP servers into Claude Code's native config.
/// Enabled servers are added via `claude mcp add-json`, disabled ones are removed.
pub fn sync_mcp_servers_to_claude(mcp_servers: &[MCPServerConfig]) -> Result<(), String> {
    for server in mcp_servers {
        if server.enabled {
            claude_mcp_add(server)?;
        } else {
            claude_mcp_remove(&server.name)?;
        }
    }
    Ok(())
}

// ─── MCP Config Generation ──────────────────────────────────────────────────

/// Generate ~/.operon/mcp-config.json from enabled MCP servers.
/// Returns the path to the config file, or None if no servers are enabled.
pub fn generate_mcp_config(mcp_servers: &[MCPServerConfig]) -> Result<Option<String>, String> {
    let enabled_servers: Vec<&MCPServerConfig> = mcp_servers.iter().filter(|s| s.enabled).collect();

    if enabled_servers.is_empty() {
        return Ok(None);
    }

    let mut servers = serde_json::Map::new();
    for server in &enabled_servers {
        let mut server_obj = serde_json::Map::new();
        server_obj.insert("command".into(), serde_json::json!(server.command));
        server_obj.insert("args".into(), serde_json::json!(server.args));
        if !server.env.is_empty() {
            server_obj.insert("env".into(), serde_json::json!(server.env));
        }
        servers.insert(server.name.clone(), serde_json::Value::Object(server_obj));
    }

    let mut config = serde_json::Map::new();
    config.insert("mcpServers".into(), serde_json::Value::Object(servers));

    let config_dir = crate::platform::data_dir();
    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create .operon directory: {}", e))?;

    let config_path = config_dir.join("mcp-config.json");
    let json = serde_json::to_string_pretty(&serde_json::Value::Object(config))
        .map_err(|e| format!("Failed to serialize MCP config: {}", e))?;
    std::fs::write(&config_path, json).map_err(|e| format!("Failed to write MCP config: {}", e))?;

    Ok(Some(config_path.to_string_lossy().to_string()))
}

/// Generate MCP config JSON string (for writing to remote hosts).
pub fn generate_mcp_config_json(mcp_servers: &[MCPServerConfig]) -> Result<Option<String>, String> {
    let enabled_servers: Vec<&MCPServerConfig> = mcp_servers.iter().filter(|s| s.enabled).collect();

    if enabled_servers.is_empty() {
        return Ok(None);
    }

    let mut servers = serde_json::Map::new();
    for server in &enabled_servers {
        let mut server_obj = serde_json::Map::new();
        server_obj.insert("command".into(), serde_json::json!(server.command));
        server_obj.insert("args".into(), serde_json::json!(server.args));
        if !server.env.is_empty() {
            server_obj.insert("env".into(), serde_json::json!(server.env));
        }
        servers.insert(server.name.clone(), serde_json::Value::Object(server_obj));
    }

    let mut config = serde_json::Map::new();
    config.insert("mcpServers".into(), serde_json::Value::Object(servers));

    let json = serde_json::to_string_pretty(&serde_json::Value::Object(config))
        .map_err(|e| format!("Failed to serialize MCP config: {}", e))?;

    Ok(Some(json))
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_mcp_catalog() -> Result<Vec<MCPCatalogEntry>, String> {
    Ok(get_research_catalog())
}

#[tauri::command]
pub async fn list_mcp_servers(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
) -> Result<Vec<MCPServerStatus>, String> {
    let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
    let catalog = get_research_catalog();

    let mut result: Vec<MCPServerStatus> = Vec::new();

    // Add configured servers
    for server in &settings.mcp_servers {
        let catalog_entry = server
            .catalog_id
            .as_ref()
            .and_then(|id| catalog.iter().find(|c| &c.id == id).cloned());
        result.push(MCPServerStatus {
            config: server.clone(),
            from_catalog: catalog_entry.is_some(),
            catalog_entry,
        });
    }

    // Add catalog entries that aren't yet configured
    for entry in &catalog {
        let already_configured = settings
            .mcp_servers
            .iter()
            .any(|s| s.catalog_id.as_ref() == Some(&entry.id));
        if !already_configured {
            result.push(MCPServerStatus {
                config: entry.config.clone(),
                from_catalog: true,
                catalog_entry: Some(entry.clone()),
            });
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn add_mcp_server(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    config: MCPServerConfig,
) -> Result<(), String> {
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;

    // Check if a server with this name already exists
    if settings.mcp_servers.iter().any(|s| s.name == config.name) {
        return Err(format!("MCP server '{}' already exists", config.name));
    }

    settings.mcp_servers.push(config);
    super::settings::SettingsManager::save_to_disk(&settings)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_mcp_server(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    name: String,
) -> Result<(), String> {
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
    settings.mcp_servers.retain(|s| s.name != name);
    super::settings::SettingsManager::save_to_disk(&settings)?;
    Ok(())
}

#[tauri::command]
pub async fn enable_mcp_server(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    name: String,
) -> Result<(), String> {
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;

    // If it's a catalog server that's not yet in settings, add it
    let exists = settings.mcp_servers.iter().any(|s| s.name == name);
    if !exists {
        let catalog = get_research_catalog();
        if let Some(entry) = catalog.iter().find(|c| c.config.name == name) {
            let mut config = entry.config.clone();
            config.enabled = true;
            settings.mcp_servers.push(config);
            super::settings::SettingsManager::save_to_disk(&settings)?;
            let _ = generate_mcp_config(&settings.mcp_servers);
            let _ = sync_mcp_servers_to_claude(&settings.mcp_servers);
            return Ok(());
        }
        return Err(format!("MCP server '{}' not found", name));
    }

    if let Some(server) = settings.mcp_servers.iter_mut().find(|s| s.name == name) {
        server.enabled = true;
    }
    super::settings::SettingsManager::save_to_disk(&settings)?;
    let _ = generate_mcp_config(&settings.mcp_servers);
    let _ = sync_mcp_servers_to_claude(&settings.mcp_servers);
    Ok(())
}

#[tauri::command]
pub async fn disable_mcp_server(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    name: String,
) -> Result<(), String> {
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
    if let Some(server) = settings.mcp_servers.iter_mut().find(|s| s.name == name) {
        server.enabled = false;
    }
    super::settings::SettingsManager::save_to_disk(&settings)?;
    let _ = generate_mcp_config(&settings.mcp_servers);
    let _ = sync_mcp_servers_to_claude(&settings.mcp_servers);
    Ok(())
}

/// Update environment variables for an MCP server (e.g. API keys).
#[tauri::command]
pub async fn update_mcp_server_env(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    name: String,
    env: HashMap<String, String>,
) -> Result<(), String> {
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
    if let Some(server) = settings.mcp_servers.iter_mut().find(|s| s.name == name) {
        server.env = env;
        super::settings::SettingsManager::save_to_disk(&settings)?;
        // Regenerate MCP config and sync to Claude Code's native config
        drop(settings);
        let settings2 = settings_state.settings.lock().map_err(|e| e.to_string())?;
        let _ = generate_mcp_config(&settings2.mcp_servers);
        let _ = sync_mcp_servers_to_claude(&settings2.mcp_servers);
        Ok(())
    } else {
        Err(format!("MCP server '{}' not found in settings", name))
    }
}

/// Install an MCP server from the catalog: check deps, add to settings, enable.
#[tauri::command]
pub async fn install_mcp_server(
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    catalog_id: String,
) -> Result<(), String> {
    let catalog = get_research_catalog();
    let entry = catalog
        .iter()
        .find(|c| c.id == catalog_id)
        .ok_or(format!("Catalog entry '{}' not found", catalog_id))?;

    // Check dependency first
    let dep_status = check_runtime(&entry.runtime).await?;
    if !dep_status.runtime_found {
        return Err(format!(
            "{} not found. Install it with: {}",
            entry.runtime, dep_status.install_hint
        ));
    }

    // Add to settings with enabled: true
    let mut settings = settings_state.settings.lock().map_err(|e| e.to_string())?;

    // Remove existing if present, then add fresh
    settings
        .mcp_servers
        .retain(|s| s.catalog_id.as_ref() != Some(&catalog_id));

    let mut config = entry.config.clone();
    config.enabled = true;
    settings.mcp_servers.push(config);

    super::settings::SettingsManager::save_to_disk(&settings)?;
    let _ = generate_mcp_config(&settings.mcp_servers);
    let _ = sync_mcp_servers_to_claude(&settings.mcp_servers);
    Ok(())
}

/// Check whether the runtime dependencies for an MCP server are satisfied.
#[tauri::command]
pub async fn check_mcp_dependencies(
    server_name: String,
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
) -> Result<DependencyStatus, String> {
    // Find the server config to determine runtime — extract data, then drop lock before await
    let runtime = {
        let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
        if let Some(server) = settings.mcp_servers.iter().find(|s| s.name == server_name) {
            infer_runtime(&server.command)
        } else {
            let catalog = get_research_catalog();
            if let Some(entry) = catalog.iter().find(|c| c.config.name == server_name) {
                entry.runtime.clone()
            } else {
                return Err(format!("Server '{}' not found", server_name));
            }
        }
    };

    check_runtime(&runtime).await
}

/// PATH prelude for every remote MCP command.
///
/// Operon's persistent SSH channel runs `bash --noprofile --norc`, so it gets
/// sshd's built-in PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) and none of the
/// user's login-shell setup. That PATH can never contain a Homebrew node/npm
/// (macOS remotes), an nvm install, or a `--prefix`ed npm global dir, so every
/// check reported "not installed" and every install ran `npm` that wasn't
/// there. Prepending the usual prefixes costs nothing and is a no-op wherever
/// they don't exist.
const REMOTE_PATH_PRELUDE: &str = "\
    export PATH=\"/opt/homebrew/bin:/usr/local/bin:$HOME/.npm-global/bin:$HOME/.local/bin:$PATH\"; \
    for _d in \"$HOME/.nvm/versions/node\"/*/bin; do \
        if [ -d \"$_d\" ]; then PATH=\"$_d:$PATH\"; fi; \
    done; export PATH; ";

/// Sentinel echoed by a remote MCP install that actually succeeded. The SSH
/// channel merges stderr into stdout and the lenient `ssh_exec` treats any
/// output as success, so `bash: npm: command not found` used to be reported to
/// the user as "installed".
const MCP_INSTALL_OK: &str = "OPERON_MCP_INSTALL_OK";

/// Check remote MCP dependencies over SSH.
#[tauri::command]
pub async fn check_remote_mcp_dependencies(
    ssh_profile: String,
    server_name: String,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
) -> Result<DependencyStatus, String> {
    // Find the runtime
    let catalog = get_research_catalog();
    let runtime = if let Some(entry) = catalog.iter().find(|c| c.config.name == server_name) {
        entry.runtime.clone()
    } else {
        return Err(format!("Server '{}' not found in catalog", server_name));
    };

    // Get SSH profile
    let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p.id == ssh_profile)
        .ok_or(format!("SSH profile '{}' not found", ssh_profile))?
        .clone();
    drop(profiles);

    let (version_cmd, min_version, linux_hint, mac_hint) = match runtime.as_str() {
        "node" => (
            "node --version",
            "20.0.0",
            "Install Node.js: curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - && sudo apt-get install -y nodejs",
            "Install Node.js on the server: brew install node",
        ),
        "python" => (
            "python3 --version",
            "3.10.0",
            "Install Python 3.10+: sudo apt-get install python3 python3-pip",
            "Install Python 3.10+ on the server: brew install python@3.12",
        ),
        _ => return Err(format!("Unknown runtime: {}", runtime)),
    };

    // Report the OS from the same call so the install hint can match it, and
    // fence the version behind a sentinel: the SSH channel merges stderr into
    // stdout, so a bare version line can't be told apart from a diagnostic.
    let check_cmd = format!(
        "{path}echo \"OPERON_OS=$(uname -s)\"; \
         _v=\"$({version_cmd} 2>/dev/null)\"; \
         if [ -n \"$_v\" ]; then echo \"OPERON_VER=$_v\"; else echo OPERON_VER=NOT_FOUND; fi",
        path = REMOTE_PATH_PRELUDE,
        version_cmd = version_cmd,
    );

    let output = super::ssh::ssh_exec(&profile, &check_cmd)
        .map_err(|e| format!("SSH check failed: {}", e))?;

    let mut remote_os = String::new();
    let mut version_str = String::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(os) = line.strip_prefix("OPERON_OS=") {
            remote_os = os.to_string();
        } else if let Some(v) = line.strip_prefix("OPERON_VER=") {
            version_str = v.to_string();
        }
    }
    let runtime_found = !version_str.contains("NOT_FOUND") && !version_str.is_empty();
    let runtime_version = if runtime_found {
        Some(version_str.replace('v', "").trim().to_string())
    } else {
        None
    };
    let install_hint = if remote_os == "Darwin" {
        mac_hint
    } else {
        linux_hint
    };

    Ok(DependencyStatus {
        satisfied: runtime_found,
        runtime: runtime.clone(),
        runtime_found,
        runtime_version,
        min_version: min_version.to_string(),
        install_hint: install_hint.to_string(),
        package_installed: false, // Can't easily check remotely
    })
}

/// Install an MCP server on a remote machine via SSH.
#[tauri::command]
pub async fn install_remote_mcp_server(
    ssh_profile: String,
    catalog_id: String,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
) -> Result<(), String> {
    let catalog = get_research_catalog();
    let entry = catalog
        .iter()
        .find(|c| c.id == catalog_id)
        .ok_or(format!("Catalog entry '{}' not found", catalog_id))?;

    let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p.id == ssh_profile)
        .ok_or(format!("SSH profile '{}' not found", ssh_profile))?
        .clone();
    drop(profiles);

    // The sentinel is emitted ONLY when the install pipeline succeeded; it is
    // the sole evidence we accept, because a failing command's output comes
    // back on stdout and would otherwise read as success.
    let pkg = entry.config.args.first().unwrap_or(&entry.config.command);
    let install_cmd = match entry.runtime.as_str() {
        "node" => format!(
            "{path}( npm install -g {pkg} 2>&1 || npx {pkg} --help >/dev/null 2>&1 ) && echo {ok}",
            path = REMOTE_PATH_PRELUDE,
            pkg = pkg,
            ok = MCP_INSTALL_OK
        ),
        "python" => format!(
            "{path}( pip install {pkg} 2>&1 || pip3 install {pkg} 2>&1 ) && echo {ok}",
            path = REMOTE_PATH_PRELUDE,
            pkg = pkg,
            ok = MCP_INSTALL_OK
        ),
        _ => return Err(format!("Unknown runtime: {}", entry.runtime)),
    };

    let out = super::ssh::ssh_exec_status(&profile, &install_cmd)
        .map_err(|e| format!("Remote install failed: {}", e))?;
    if !out.stdout.contains(MCP_INSTALL_OK) {
        let detail = out.diagnostic();
        return Err(if detail.is_empty() {
            format!(
                "Remote install of '{}' failed (exit status {}) with no output — is {} installed on the server?",
                pkg, out.code, entry.runtime
            )
        } else {
            format!("Remote install of '{}' failed: {}", pkg, detail)
        });
    }

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn infer_runtime(command: &str) -> String {
    match command {
        "node" | "npx" | "npm" => "node".to_string(),
        "python" | "python3" | "pip" | "pip3" | "uvx" | "uv" => "python".to_string(),
        _ => "unknown".to_string(),
    }
}

async fn check_runtime(runtime: &str) -> Result<DependencyStatus, String> {
    let python_cmd = crate::platform::python_command();
    let python_install_hint = if cfg!(target_os = "windows") {
        "winget install Python.Python.3.12"
    } else if cfg!(target_os = "macos") {
        "brew install python@3.12"
    } else {
        "sudo apt install python3 python3-pip"
    };
    let node_install_hint = if cfg!(target_os = "windows") {
        "winget install OpenJS.NodeJS.LTS"
    } else if cfg!(target_os = "macos") {
        "brew install node"
    } else {
        "sudo apt install nodejs"
    };

    let (cmd, min_version, install_hint) = match runtime {
        "node" => ("node", "20.0.0", node_install_hint),
        "python" => (python_cmd, "3.10.0", python_install_hint),
        _ => {
            return Ok(DependencyStatus {
                satisfied: false,
                runtime: runtime.to_string(),
                runtime_found: false,
                runtime_version: None,
                min_version: "unknown".to_string(),
                install_hint: format!("Unknown runtime: {}", runtime),
                package_installed: false,
            })
        }
    };

    // Resolve via the platform tool-discovery (`where.exe` on Windows, `which`
    // elsewhere). A bare `Command::new("node")` misses `.cmd`/PATHEXT shims and
    // ignores Operon's augmented tool PATH.
    match crate::platform::check_tool(cmd) {
        Some((_, version_raw)) => {
            let version = version_raw.trim().replace('v', "").replace("Python ", "");
            Ok(DependencyStatus {
                satisfied: true,
                runtime: runtime.to_string(),
                runtime_found: true,
                runtime_version: Some(version),
                min_version: min_version.to_string(),
                install_hint: install_hint.to_string(),
                package_installed: true,
            })
        }
        None => Ok(DependencyStatus {
            satisfied: false,
            runtime: runtime.to_string(),
            runtime_found: false,
            runtime_version: None,
            min_version: min_version.to_string(),
            install_hint: install_hint.to_string(),
            package_installed: false,
        }),
    }
}
