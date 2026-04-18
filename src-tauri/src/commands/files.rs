use base64::Engine;
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub extension: Option<String>,
}

#[tauri::command]
pub async fn list_directory(
    path: String,
    show_hidden: Option<bool>,
) -> Result<Vec<FileEntry>, String> {
    let show_hidden = show_hidden.unwrap_or(false);
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(&path).map_err(|e| e.to_string())?;

    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files unless requested (platform-aware: dot-prefix on Unix, FILE_ATTRIBUTE_HIDDEN on Windows)
        if !show_hidden && crate::platform::is_hidden(&entry_path) {
            continue;
        }

        // Use std::fs::metadata which follows symlinks (resolves to target).
        // Fall back to symlink_metadata if the target doesn't exist (broken symlink).
        let metadata = match std::fs::metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => match std::fs::symlink_metadata(&entry_path) {
                Ok(m) => m,
                Err(_) => continue,
            },
        };

        entries.push(FileEntry {
            name: name.clone(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            extension: entry_path
                .extension()
                .map(|e| e.to_string_lossy().to_string()),
        });
    }

    // Directories first, then files, both alphabetical
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {}", path, e))
}

#[tauri::command]
pub async fn read_file_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| format!("Failed to write {}: {}", path, e))
}

/// Save base64-encoded image data (from clipboard paste) to a temp file.
/// Returns the absolute path to the saved file.
#[tauri::command]
pub async fn save_clipboard_image(data: String, extension: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let tmp_dir = crate::platform::temp_dir().join("operon-clipboard");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let filename = format!("clipboard-{}.{}", timestamp, extension);
    let path = tmp_dir.join(&filename);

    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write clipboard image: {}", e))?;

    Ok(path.to_string_lossy().to_string())
}

/// Save any file attachment (from file picker) to a temp directory.
/// Accepts base64-encoded file data and the original filename.
/// Returns the absolute path to the saved file.
#[tauri::command]
pub async fn save_attachment_file(data: String, filename: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let tmp_dir = std::env::temp_dir().join("operon-attachments");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    // Preserve original filename but prefix with timestamp to avoid collisions
    let safe_name = format!("{}-{}", timestamp, filename.replace(['/', '\\'], "_"));
    let path = tmp_dir.join(&safe_name);

    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write attachment file: {}", e))?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_home_dir() -> Result<String, String> {
    crate::platform::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not determine home directory".to_string())
}

#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, "").map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_path(path: String) -> Result<(), String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn rename_path(old_path: String, new_path: String) -> Result<(), String> {
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())
}

// --- Project File Index ---

#[derive(Serialize, Clone)]
pub struct IndexEntry {
    pub path: String, // relative path from project root
    pub size: u64,
    pub is_dir: bool,
    pub extension: Option<String>,
}

/// Recursively index a local project directory.
/// Returns a flat list of files/folders with relative paths.
#[tauri::command]
pub async fn index_project(root_path: String) -> Result<Vec<IndexEntry>, String> {
    let root = std::path::Path::new(&root_path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", root_path));
    }

    let skip_dirs: std::collections::HashSet<&str> = [
        ".git",
        "node_modules",
        "__pycache__",
        ".operon-run",
        ".next",
        ".venv",
        "venv",
        ".tox",
        ".mypy_cache",
        "target",
        "build",
        "dist",
        ".cache",
        ".eggs",
    ]
    .into_iter()
    .collect();

    let mut entries = Vec::new();
    index_walk(root, root, 0, 4, &skip_dirs, &mut entries);

    // Cap at 300 entries
    entries.truncate(300);
    Ok(entries)
}

fn index_walk(
    base: &std::path::Path,
    dir: &std::path::Path,
    depth: usize,
    max_depth: usize,
    skip_dirs: &std::collections::HashSet<&str>,
    entries: &mut Vec<IndexEntry>,
) {
    if depth > max_depth {
        return;
    }
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    let mut children: Vec<_> = read_dir.flatten().collect();
    children.sort_by_key(|a| a.file_name());

    for entry in children {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs (platform-aware)
        if crate::platform::is_hidden(&path) {
            continue;
        }

        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let rel_path = path
            .strip_prefix(base)
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());

        if metadata.is_dir() {
            if skip_dirs.contains(name.as_str()) {
                continue;
            }

            // Count children to show summary for large dirs
            let child_count = std::fs::read_dir(&path).map(|rd| rd.count()).unwrap_or(0);

            entries.push(IndexEntry {
                path: format!("{}/", rel_path),
                size: child_count as u64, // repurpose size as item count for dirs
                is_dir: true,
                extension: None,
            });

            // Only recurse into dirs with < 200 items (skip massive data dirs)
            if child_count < 200 {
                index_walk(base, &path, depth + 1, max_depth, skip_dirs, entries);
            }
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());

            entries.push(IndexEntry {
                path: rel_path,
                size: metadata.len(),
                is_dir: false,
                extension: ext,
            });
        }

        // Hard cap to prevent runaway indexing
        if entries.len() >= 300 {
            return;
        }
    }
}

/// Index a remote project directory via SSH.
#[tauri::command]
pub async fn index_remote_project(
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    profile_id: String,
    remote_path: String,
) -> Result<Vec<IndexEntry>, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Single SSH call: find with maxdepth, output relative path + size + type
    let find_cmd = format!(
        "cd '{}' && find . -maxdepth 4 -not -path '*/\\.*' \
         -not -path '*/node_modules/*' -not -path '*/__pycache__/*' \
         -not -path '*/target/*' -not -path '*/.git/*' \
         -printf '%s\\t%y\\t%P\\n' 2>/dev/null | head -300",
        remote_path.replace('\'', "'\\''")
    );

    let output = crate::commands::ssh::ssh_exec(&profile, &find_cmd)?;
    let mut entries = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let size: u64 = parts[0].parse().unwrap_or(0);
        let ftype = parts[1];
        let rel_path = parts[2].to_string();
        if rel_path.is_empty() {
            continue;
        }

        let is_dir = ftype == "d";
        let extension = if !is_dir {
            rel_path.rsplit('.').next().and_then(|e| {
                if e != rel_path {
                    Some(e.to_lowercase())
                } else {
                    None
                }
            })
        } else {
            None
        };

        entries.push(IndexEntry {
            path: if is_dir {
                format!("{}/", rel_path)
            } else {
                rel_path
            },
            size,
            is_dir,
            extension,
        });
    }

    Ok(entries)
}

// --- Protocol System ---
//
// Protocols can be:
//   1. A FOLDER with PROTOCOL.md as the entry point (supports sub-files: references/, assets/, scripts/)
//   2. A single .md file (simple protocol, backward compatible)
//
// Directory structure for folder-based protocol:
//   protocols/
//     scanpy/
//       PROTOCOL.md          <- entry point (required)
//       assets/
//         analysis_template.py
//       references/
//         api_reference.md
//         plotting_guide.md
//       scripts/
//         qc_analysis.py
//
// The entry point PROTOCOL.md can reference sub-files. When activated, the system
// reads PROTOCOL.md and also provides a manifest of all files in the folder so
// Claude knows what resources are available.

#[derive(Serialize, Clone)]
pub struct ProtocolEntry {
    pub id: String,          // folder name or filename without .md
    pub name: String,        // display name (from H1 header or derived from id)
    pub description: String, // first non-header line from entry point
    pub path: String,        // full path to entry point .md file
    pub is_folder: bool,     // true if folder-based, false if single .md file
    pub file_count: usize,   // number of files in the protocol (1 for single .md)
    pub source: String,      // "bundled" or "user"
    pub category: String,    // auto-detected category for grouping
}

/// Auto-detect a protocol category from its id and content.
/// Categories: database, pipeline, writing, visualization, integration, genomics,
///             cheminformatics, ml_ai, statistics, tool, other
fn detect_category(id: &str, _content: &str) -> String {
    let id = id.to_lowercase();

    // --- Databases & References (match first — very explicit naming) ---
    if id.ends_with("-database")
        || id.contains("database")
        || id == "openalex-database"
        || id == "depmap"
    {
        return "database".to_string();
    }

    // --- Writing, Documents & Publishing ---
    if id.contains("writing")
        || id.contains("docx")
        || id.contains("pptx")
        || id.contains("xlsx")
        || id.contains("pdf")
        || id.contains("latex")
        || id.contains("poster")
        || id.contains("slide")
        || id.contains("paper-2-web")
        || id.contains("literature-review")
        || id.contains("peer-review")
        || id.contains("citation")
        || id.contains("infographic")
        || id.contains("venue-template")
        || id.contains("markdown")
        || id.contains("report")
        || id.contains("research-grant")
        || id.contains("scientific-writing")
        || id.contains("scientific-slide")
        || id.contains("scientific-schemat")
        || id.contains("open-notebook")
        || id.contains("clinical-report")
        || id.contains("markitdown")
    {
        return "writing".to_string();
    }

    // --- Visualization & Plotting ---
    if id.contains("volcano")
        || id.contains("plot")
        || id.contains("heatmap")
        || id.contains("visualization")
        || id.contains("matplotlib")
        || id.contains("seaborn")
        || id.contains("plotly")
        || id.contains("generate-image")
        || id.contains("umap-learn")
    {
        return "visualization".to_string();
    }

    // --- Lab Integrations & Platforms ---
    if id.contains("integration")
        || id.contains("latchbio")
        || id.contains("benchling")
        || id.contains("dnanexus")
        || id.contains("omero")
        || id.contains("opentrons")
        || id.contains("ginkgo")
        || id.contains("labarchive")
        || id.contains("protocolsio")
        || id.contains("pylabrobot")
        || id.contains("rowan")
        || id.contains("modal")
        || id.contains("denario")
        || id.contains("adaptyv")
    {
        return "integration".to_string();
    }

    // --- Genomics & Omics Analysis Pipelines ---
    if id.contains("pipeline")
        || id.contains("seq-analysis")
        || id.contains("rnaseq")
        || id.contains("atacseq")
        || id.contains("spatial-transcriptomics")
        || id.contains("scrna")
        || id.contains("bulk-rna")
        || id.contains("scvelo")
        || id.contains("gwas")
        || id.contains("phylogenetic")
        || id.contains("neuropixel")
        || id.contains("metabolomics")
        || id.contains("glycoengineering")
        || id.contains("molecular-dynamics")
        || id.contains("scanpy")
        || id.contains("anndata")
        || id.contains("pydeseq")
        || id.contains("pysam")
        || id.contains("scvi")
        || id.contains("cellxgene")
        || id.contains("lamindb")
        || id.contains("scikit-bio")
        || id.contains("deeptools")
        || id.contains("flowio")
        || id.contains("pathml")
        || id.contains("histolab")
        || id.contains("tiledbvcf")
        || id.contains("gtars")
        || id.contains("geniml")
        || id.contains("polars-bio")
        || id.contains("etetoolkit")
        || id.contains("biopython")
        || id.contains("bioservices")
        || id.contains("gget")
        || id.contains("pyopenms")
        || id.contains("matchms")
        || id.contains("arboreto")
        || id.contains("neurokit")
        || id.contains("pydicom")
        || id.contains("imaging-data")
        || id.contains("get-available-resources")
    {
        return "genomics".to_string();
    }

    // --- Cheminformatics & Drug Discovery ---
    if id.contains("rdkit")
        || id.contains("deepchem")
        || id.contains("diffdock")
        || id.contains("datamol")
        || id.contains("molfeat")
        || id.contains("medchem")
        || id.contains("torchdrug")
        || id.contains("esm")
        || id.contains("alphafold")
        || id.contains("dhdna")
        || id.contains("pytdc")
        || id.contains("primekg")
        || id.contains("cobrapy")
        || id.contains("pymatgen")
    {
        return "cheminformatics".to_string();
    }

    // --- ML, AI & Quantum Computing ---
    if id.contains("transformers")
        || id.contains("pytorch")
        || id.contains("torch-geometric")
        || id.contains("scikit-learn")
        || id.contains("stable-baselines")
        || id.contains("pennylane")
        || id.contains("qiskit")
        || id.contains("qutip")
        || id.contains("cirq")
        || id.contains("shap")
        || id.contains("pufferlib")
        || id.contains("hypogenic")
        || id.contains("timesfm")
        || id.contains("aeon")
        || id.contains("pymc")
        || id.contains("scikit-survival")
    {
        return "ml_ai".to_string();
    }

    // --- Statistics & Data Science ---
    if id.contains("statsmodels")
        || id.contains("statistical")
        || id.contains("polars")
        || id.contains("dask")
        || id.contains("vaex")
        || id.contains("zarr")
        || id.contains("sympy")
        || id.contains("simpy")
        || id.contains("pymoo")
        || id.contains("networkx")
        || id.contains("exploratory-data")
        || id.contains("matlab")
        || id.contains("geopandas")
        || id.contains("fluidsim")
        || id.contains("astropy")
        || id.contains("geomaster")
    {
        return "statistics".to_string();
    }

    // --- Research & Reasoning ---
    if id.contains("hypothesis")
        || id.contains("brainstorming")
        || id.contains("critical-thinking")
        || id.contains("scholar-evaluation")
        || id.contains("consciousness")
        || id.contains("what-if")
        || id.contains("research-lookup")
        || id.contains("bgpt-paper")
        || id.contains("perplexity-search")
        || id.contains("parallel-web")
        || id.contains("pyzotero")
    {
        return "research".to_string();
    }

    // --- Clinical & Healthcare ---
    if id.contains("clinical")
        || id.contains("treatment")
        || id.contains("pyhealth")
        || id.contains("iso-13485")
    {
        return "clinical".to_string();
    }

    // --- Finance & Business ---
    if id.contains("alpha-vantage")
        || id.contains("hedgefund")
        || id.contains("edgartools")
        || id.contains("fred-economic")
        || id.contains("usfiscaldata")
        || id.contains("market-research")
        || id.contains("datacommons")
    {
        return "finance".to_string();
    }

    // Catch-all: anything with "import " or "pip install" in content is likely a tool
    // But we avoid reading content for performance with 180+ protocols
    "other".to_string()
}

/// Get the protocols directory, creating it if needed.
fn protocols_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::platform::data_dir().join("protocols");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create protocols dir: {}", e))?;
    }
    Ok(dir)
}

/// Convert a folder/file name like "scrna-seq-analysis" → "Scrna Seq Analysis"
fn id_to_display_name(id: &str) -> String {
    id.split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.collect::<String>()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract description: first non-empty, non-header line from the markdown.
fn extract_description(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let clean = trimmed.trim_start_matches(['*', '_', '-', '>', ' ']);
        if !clean.is_empty() {
            let desc = if clean.len() > 120 {
                format!(
                    "{}...",
                    &clean[..clean
                        .char_indices()
                        .nth(120)
                        .map(|(i, _)| i)
                        .unwrap_or(clean.len())]
                )
            } else {
                clean.to_string()
            };
            return desc;
        }
    }
    "No description".to_string()
}

/// Count all files recursively in a directory.
fn count_files_recursive(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}

/// List all files in a directory recursively, returning relative paths.
fn list_files_recursive(base: &std::path::Path, dir: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(list_files_recursive(base, &path));
            } else {
                if let Ok(rel) = path.strip_prefix(base) {
                    files.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
    files.sort();
    files
}

/// Scan a directory for protocols (both folder-based and single-file).
fn scan_protocols_in_dir(
    dir: &std::path::Path,
    protocols: &mut Vec<ProtocolEntry>,
    seen_ids: &mut std::collections::HashSet<String>,
    source: &str,
) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name_str = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            // Folder-based protocol: look for PROTOCOL.md or SKILL.md entry point
            let entry_point = if path.join("PROTOCOL.md").exists() {
                path.join("PROTOCOL.md")
            } else if path.join("SKILL.md").exists() {
                path.join("SKILL.md")
            } else {
                continue; // Not a protocol folder — skip
            };
            let id = name_str.clone();
            if seen_ids.contains(&id) {
                continue;
            }
            seen_ids.insert(id.clone());

            let content = std::fs::read_to_string(&entry_point).unwrap_or_default();
            let display_name = content
                .lines()
                .find(|l| l.starts_with("# "))
                .map(|l| l.trim_start_matches("# ").trim().to_string())
                .unwrap_or_else(|| id_to_display_name(&id));
            let description = extract_description(&content);
            let file_count = count_files_recursive(&path);

            let category = detect_category(&id, &content);
            protocols.push(ProtocolEntry {
                id,
                name: display_name,
                description,
                path: entry_point.to_string_lossy().to_string(),
                is_folder: true,
                file_count,
                source: source.to_string(),
                category,
            });
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            // Single-file protocol
            let id = name_str
                .strip_suffix(".md")
                .unwrap_or(&name_str)
                .to_string();
            if seen_ids.contains(&id) {
                continue;
            }
            seen_ids.insert(id.clone());

            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let display_name = content
                .lines()
                .find(|l| l.starts_with("# "))
                .map(|l| l.trim_start_matches("# ").trim().to_string())
                .unwrap_or_else(|| id_to_display_name(&id));
            let description = extract_description(&content);

            let category = detect_category(&id, &content);
            protocols.push(ProtocolEntry {
                id,
                name: display_name,
                description,
                path: path.to_string_lossy().to_string(),
                is_folder: false,
                file_count: 1,
                source: source.to_string(),
                category,
            });
        }
    }
}

#[tauri::command]
pub async fn list_protocols(app_handle: tauri::AppHandle) -> Result<Vec<ProtocolEntry>, String> {
    let user_dir = protocols_dir()?;
    let mut protocols = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    // 1. User protocols first (take priority)
    scan_protocols_in_dir(&user_dir, &mut protocols, &mut seen_ids, "user");

    // 2. Bundled protocols — use Tauri's resource resolver (works in both dev and built app)
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        let bundled = resource_dir.join("protocols");
        if bundled.is_dir() {
            scan_protocols_in_dir(&bundled, &mut protocols, &mut seen_ids, "bundled");
        }
    }

    // 3. Fallback: look relative to executable for macOS bundle and dev mode
    if let Ok(exe_path) = std::env::current_exe() {
        // macOS bundle: Operon.app/Contents/MacOS/operon → ../../Resources/protocols
        if let Some(resources) = exe_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("Resources").join("protocols"))
        {
            if resources.is_dir() {
                scan_protocols_in_dir(&resources, &mut protocols, &mut seen_ids, "bundled");
            }
        }

        // Dev mode: executable is at src-tauri/target/debug/operon
        // Walk up to find protocols/ folder. Prefer src-tauri/protocols/ (full set)
        // over root protocols/ (may be a smaller subset).
        let mut dir = exe_path.parent();
        for _ in 0..6 {
            if let Some(d) = dir {
                // Check src-tauri/protocols first (the primary bundled location)
                let src_tauri = d.join("src-tauri").join("protocols");
                if src_tauri.is_dir() {
                    scan_protocols_in_dir(&src_tauri, &mut protocols, &mut seen_ids, "bundled");
                    break;
                }
                // Fallback to root protocols/
                let candidate = d.join("protocols");
                if candidate.is_dir() {
                    scan_protocols_in_dir(&candidate, &mut protocols, &mut seen_ids, "bundled");
                    break;
                }
                dir = d.parent();
            } else {
                break;
            }
        }
    }

    protocols.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(protocols)
}

/// Read a protocol's full context. For folder-based protocols, this includes:
/// - The PROTOCOL.md content
/// - A manifest of all available files in the folder
/// - Contents of all .md files in references/
/// For single-file protocols, just returns the file content.
#[tauri::command]
pub async fn read_protocol(
    app_handle: tauri::AppHandle,
    protocol_id: String,
) -> Result<String, String> {
    let user_dir = protocols_dir()?;

    // Helper: check if a dir has PROTOCOL.md or SKILL.md
    let has_entry_point = |dir: &std::path::Path| -> bool {
        dir.join("PROTOCOL.md").exists() || dir.join("SKILL.md").exists()
    };

    // Check user dir first, then bundled
    let mut protocol_path: Option<std::path::PathBuf> = None;

    // Folder-based in user dir?
    let folder_path = user_dir.join(&protocol_id);
    if folder_path.is_dir() && has_entry_point(&folder_path) {
        protocol_path = Some(folder_path);
    }

    // Single .md in user dir?
    if protocol_path.is_none() {
        let md_path = user_dir.join(format!("{}.md", protocol_id));
        if md_path.exists() {
            return std::fs::read_to_string(&md_path)
                .map_err(|e| format!("Failed to read protocol: {}", e));
        }
    }

    // Check bundled dirs if not found in user dir
    if protocol_path.is_none() {
        // Collect all possible search dirs
        let mut search_dirs: Vec<std::path::PathBuf> = Vec::new();

        // Tauri resource resolver (works in both dev and production)
        if let Ok(resource_dir) = app_handle.path().resource_dir() {
            let bundled = resource_dir.join("protocols");
            if bundled.is_dir() {
                search_dirs.push(bundled);
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            // macOS bundle fallback: Resources/protocols
            if let Some(resources) = exe_path
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("Resources").join("protocols"))
            {
                if resources.is_dir() {
                    search_dirs.push(resources);
                }
            }
            // Dev mode: walk up to find project protocols/
            let mut dir = exe_path.parent();
            for _ in 0..6 {
                if let Some(d) = dir {
                    let candidate = d.join("protocols");
                    if candidate.is_dir() {
                        search_dirs.push(candidate);
                        break;
                    }
                    dir = d.parent();
                } else {
                    break;
                }
            }
        }

        for search_dir in &search_dirs {
            let fp = search_dir.join(&protocol_id);
            if fp.is_dir() && has_entry_point(&fp) {
                protocol_path = Some(fp);
                break;
            }
            let mp = search_dir.join(format!("{}.md", protocol_id));
            if mp.exists() {
                return std::fs::read_to_string(&mp)
                    .map_err(|e| format!("Failed to read protocol: {}", e));
            }
        }
    }

    // If we found a folder-based protocol, assemble full context
    if let Some(folder) = protocol_path {
        let entry_point = if folder.join("PROTOCOL.md").exists() {
            folder.join("PROTOCOL.md")
        } else {
            folder.join("SKILL.md")
        };
        let main_content = std::fs::read_to_string(&entry_point)
            .map_err(|e| format!("Failed to read protocol entry point: {}", e))?;

        let all_files = list_files_recursive(&folder, &folder);
        let manifest = all_files
            .iter()
            .map(|f| format!("  {}", f))
            .collect::<Vec<_>>()
            .join("\n");

        let mut full_context = format!(
            "{}\n\n--- Protocol File Manifest ---\nAvailable files in this protocol:\n{}\n",
            main_content, manifest
        );

        // Auto-include all .md files from references/ subdirectory
        let refs_dir = folder.join("references");
        if refs_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&refs_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            let rel_name = p
                                .strip_prefix(&folder)
                                .map(|r| r.to_string_lossy().to_string())
                                .unwrap_or_default();
                            full_context.push_str(&format!(
                                "\n--- Reference: {} ---\n{}\n",
                                rel_name, content
                            ));
                        }
                    }
                }
            }
        }

        return Ok(full_context);
    }

    Err(format!("Protocol '{}' not found", protocol_id))
}

#[tauri::command]
pub async fn get_protocols_dir() -> Result<String, String> {
    protocols_dir().map(|p| p.to_string_lossy().to_string())
}

/// Save a protocol as a single .md file in ~/.operon/protocols/.
/// If `protocol_id` already exists, it overwrites.
#[tauri::command]
pub async fn save_protocol(protocol_id: String, content: String) -> Result<(), String> {
    let dir = protocols_dir()?;

    // Sanitize ID: only allow alphanumeric, hyphens, underscores
    let sanitized: String = protocol_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        return Err("Protocol ID cannot be empty".into());
    }

    let file_path = dir.join(format!("{}.md", sanitized));
    std::fs::write(&file_path, content).map_err(|e| format!("Failed to save protocol: {}", e))
}

/// Delete a user-created protocol (single .md or folder) from ~/.operon/protocols/.
#[tauri::command]
pub async fn delete_protocol(protocol_id: String) -> Result<(), String> {
    let dir = protocols_dir()?;

    // Try single .md file first
    let md_path = dir.join(format!("{}.md", protocol_id));
    if md_path.exists() {
        return std::fs::remove_file(&md_path)
            .map_err(|e| format!("Failed to delete protocol: {}", e));
    }

    // Try folder-based protocol
    let folder_path = dir.join(&protocol_id);
    if folder_path.is_dir() {
        return std::fs::remove_dir_all(&folder_path)
            .map_err(|e| format!("Failed to delete protocol folder: {}", e));
    }

    Err(format!(
        "Protocol '{}' not found in user protocols",
        protocol_id
    ))
}

/// Generate a protocol using Claude Code in one-shot mode.
/// Returns the generated markdown content.
#[tauri::command]
pub async fn generate_protocol(description: String) -> Result<String, String> {
    let meta_prompt = format!(
        r#"You are generating a protocol for a bioinformatics AI coding assistant called Operon. \
The protocol will be injected into every prompt when active, guiding Claude on how to handle tasks in a specific domain.

The user wants a protocol for: {}

Generate a complete, well-structured protocol in Markdown format. The protocol MUST include:

1. A title as an H1 header (# Protocol Name)
2. A brief description of what this protocol covers
3. Key rules and constraints the AI should follow
4. Recommended tools, packages, or software with version preferences
5. Common patterns, templates, or code snippets
6. Error handling and troubleshooting guidance
7. Best practices specific to this domain

If relevant to bioinformatics, include sections for:
- Environment setup (conda, modules, containers)
- SLURM/HPC job submission patterns
- Data format expectations (FASTQ, BAM, VCF, h5ad, etc.)
- Quality control checkpoints
- Reproducibility guidelines

Output ONLY the protocol markdown — no preamble, no explanation, no code fences wrapping the whole thing. Start directly with the # header."#,
        description
    );

    // Escape single quotes in the prompt for shell safety
    let escaped_prompt = meta_prompt.replace('\'', "'\\''");

    // Run through user's login shell (zsh on macOS) so PATH includes Homebrew/Node
    let shell_cmd = format!(
        "claude -p '{}' --max-turns 1 --output-format text",
        escaped_prompt
    );

    let output = crate::platform::shell_exec_async(&shell_cmd)
        .output()
        .await
        .map_err(|e| format!("Failed to run Claude: {}. Is Claude Code installed?", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude returned an error: {}", stderr.trim()));
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if result.is_empty() {
        return Err("Claude returned empty output".into());
    }

    Ok(result)
}

/// Generate a protocol from selected pipeline files.
/// Takes an array of (filename, content) pairs and optional user context,
/// sends them to Claude in one-shot mode to produce a PROTOCOL.md.
#[tauri::command]
pub async fn generate_protocol_from_files(
    file_contents: Vec<(String, String)>,
    context: Option<String>,
) -> Result<String, String> {
    // Build the file context section
    let mut files_section = String::new();
    for (name, content) in &file_contents {
        // Truncate very large files to avoid exceeding context limits
        let truncated = if content.len() > 50_000 {
            format!("{}... [truncated, {} total chars]", &content[..50_000], content.len())
        } else {
            content.clone()
        };
        files_section.push_str(&format!(
            "<file name=\"{}\">\n{}\n</file>\n\n",
            name, truncated
        ));
    }

    let context_line = context
        .filter(|c| !c.trim().is_empty())
        .map(|c| format!("\nThe user describes this pipeline as: {}\n", c))
        .unwrap_or_default();

    let meta_prompt = format!(
        r#"You are analyzing an existing pipeline/workflow to generate a protocol for Operon, a bioinformatics AI coding assistant. \
The protocol will be injected into every prompt when active, guiding Claude on how to handle tasks with this pipeline.
{context}
Below are the key files from the pipeline. Analyze them carefully to understand the workflow, tools, dependencies, and patterns.

{files}
Based on these files, generate a complete protocol in Markdown format. The protocol MUST include:

1. A title as an H1 header (# Pipeline Name)
2. A brief description of what this pipeline does, its purpose and scope
3. Prerequisites: required tools, packages, modules, conda environments
4. Input files: expected formats, naming conventions, directory structure
5. Step-by-step workflow: each stage of the pipeline in order, with the commands or scripts involved
6. Configuration: key parameters, config files, and how to customize them
7. Output files: what gets produced, where, and in what format
8. SLURM/HPC patterns: if detected, include job submission templates, resource requirements
9. Common issues and troubleshooting tips based on patterns in the code
10. Best practices extracted from the code (error handling, logging, checkpoints)

Make the protocol actionable — when a user activates it and asks Claude to "run the pipeline" or "process new samples", \
Claude should have enough context to execute it correctly on their system.

Output ONLY the protocol markdown — no preamble, no explanation, no code fences wrapping the whole thing. Start directly with the # header."#,
        context = context_line,
        files = files_section,
    );

    // Use base64 encoding to safely pass the potentially large prompt through the shell
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, meta_prompt.as_bytes());
    let shell_cmd = format!(
        "echo '{}' | base64 -d | claude -p - --max-turns 1 --output-format text",
        encoded
    );

    let output = crate::platform::shell_exec_async(&shell_cmd)
        .output()
        .await
        .map_err(|e| format!("Failed to run Claude: {}. Is Claude Code installed?", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude returned an error: {}", stderr.trim()));
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if result.is_empty() {
        return Err("Claude returned empty output".into());
    }

    Ok(result)
}

// --- Search (ripgrep-based, with grep / Rust-walker fallback) ---

#[derive(Serialize, Clone)]
pub struct SearchHit {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Serialize, Clone)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// Which backend handled the query — "ripgrep-sidecar", "ripgrep-system",
    /// "ripgrep-remote", "grep-remote", "rust-walker". Surfaced in the UI.
    pub backend: String,
}

/// Directories skipped during recursive search.
const SKIP_SEARCH_DIRS: &[&str] = &[
    ".git", "node_modules", "__pycache__", ".next", ".venv", "venv",
    ".tox", ".mypy_cache", "target", "build", "dist", ".cache", ".eggs",
    ".operon-run",
];

/// Cap the displayed text on any one match line so a single absurd line can't
/// blow up the IPC payload.
fn truncate_match_text(s: &str) -> String {
    if s.len() > 400 {
        let mut end = 400;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
//  Local search — prefers bundled ripgrep sidecar, falls back to PATH rg,
//  finally falls back to a simple Rust walker so search never breaks.
// ---------------------------------------------------------------------------

/// Try to run the bundled sidecar ripgrep. Returns Ok(stdout_bytes) on exit
/// codes 0 (matches) or 1 (no matches); Err on spawn failure or exit 2.
async fn run_rg_sidecar(
    app: &tauri::AppHandle,
    args: &[String],
) -> Result<(Vec<u8>, i32), String> {
    use tauri_plugin_shell::ShellExt;
    let shell = app.shell();
    let sidecar = shell
        .sidecar("rg")
        .map_err(|e| format!("sidecar unavailable: {}", e))?;
    let output = sidecar
        .args(args)
        .output()
        .await
        .map_err(|e| format!("sidecar spawn failed: {}", e))?;
    let code = output.status.code().unwrap_or(-1);
    if code == 0 || code == 1 {
        Ok((output.stdout, code))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("rg exited {}: {}", code, err.trim()))
    }
}

/// Same idea, but via the user's system PATH (for dev builds where the
/// sidecar isn't available). Uses the platform's login shell so PATH is
/// inherited like the user's terminal.
async fn run_rg_system(args: &[String]) -> Result<(Vec<u8>, i32), String> {
    // Quote each arg for safe passthrough. ripgrep accepts `--` to separate
    // flags from patterns, so this is fine even for pattern-like args.
    let shell_cmd = std::iter::once("rg".to_string())
        .chain(args.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))))
        .collect::<Vec<_>>()
        .join(" ");
    let output = crate::platform::shell_exec_async(&shell_cmd)
        .output()
        .await
        .map_err(|e| format!("spawn failed: {}", e))?;
    let code = output.status.code().unwrap_or(-1);
    if code == 0 || code == 1 {
        Ok((output.stdout, code))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("rg exited {}: {}", code, err.trim()))
    }
}

/// Build ripgrep arguments for a text search.
fn build_rg_args(
    query: &str,
    root_path: &str,
    case_sensitive: bool,
    use_regex: bool,
    max_results: usize,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    args.push("--json".into());
    args.push("-n".into());
    args.push("--max-count".into());
    // Per-file cap so one pathological file can't dominate the result set.
    args.push(format!("{}", (max_results / 4).clamp(20, 500)));
    args.push("--max-filesize".into());
    args.push("1M".into());
    args.push("--no-require-git".into());
    if case_sensitive {
        args.push("--case-sensitive".into());
    } else {
        args.push("--ignore-case".into());
    }
    if !use_regex {
        args.push("--fixed-strings".into());
    }
    // Skip common build/cache dirs the user doesn't want cluttering results.
    for d in SKIP_SEARCH_DIRS {
        args.push("--glob".into());
        args.push(format!("!**/{}/**", d));
    }
    args.push("--".into());
    args.push(query.to_string());
    args.push(root_path.to_string());
    args
}

/// Parse ripgrep's NDJSON `--json` stream, collecting at most `max_results`
/// hits. Paths returned are relative to `root_path` when possible.
fn parse_rg_json(bytes: &[u8], root_path: &str, max_results: usize) -> Vec<SearchHit> {
    let text = String::from_utf8_lossy(bytes);
    let mut hits = Vec::new();
    for line in text.lines() {
        if hits.len() >= max_results {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }
        let data = match v.get("data") {
            Some(d) => d,
            None => continue,
        };
        let abs_path = data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");
        if abs_path.is_empty() {
            continue;
        }
        let rel_path = match std::path::Path::new(abs_path).strip_prefix(root_path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => abs_path.to_string(),
        };
        let text = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim_end_matches('\n');
        let line_number = data
            .get("line_number")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32;

        hits.push(SearchHit {
            path: rel_path,
            line: line_number,
            text: truncate_match_text(text),
        });
    }
    hits
}

/// Rust-walker fallback when neither sidecar nor system rg is available.
/// Kept intentionally simple — only runs when ripgrep can't be found at all.
fn search_walker_fallback(
    root: &std::path::Path,
    query: &str,
    case_sensitive: bool,
    max_results: usize,
) -> Vec<SearchHit> {
    fn walk(
        base: &std::path::Path,
        dir: &std::path::Path,
        depth: usize,
        max_depth: usize,
        q_lower: &str,
        cs: bool,
        q_raw: &str,
        hits: &mut Vec<SearchHit>,
        max: usize,
    ) {
        if hits.len() >= max || depth > max_depth {
            return;
        }
        let rd = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => return,
        };
        for entry in rd.flatten() {
            if hits.len() >= max {
                return;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if crate::platform::is_hidden(&path) {
                continue;
            }
            let md = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.is_dir() {
                if SKIP_SEARCH_DIRS.contains(&name.as_str()) {
                    continue;
                }
                walk(base, &path, depth + 1, max_depth, q_lower, cs, q_raw, hits, max);
                continue;
            }
            if md.len() > 1_000_000 {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let rel = path
                .strip_prefix(base)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            for (idx, line) in content.lines().enumerate() {
                let m = if cs {
                    line.contains(q_raw)
                } else {
                    line.to_lowercase().contains(q_lower)
                };
                if m {
                    hits.push(SearchHit {
                        path: rel.clone(),
                        line: (idx + 1) as u32,
                        text: truncate_match_text(line),
                    });
                    if hits.len() >= max {
                        return;
                    }
                }
            }
        }
    }

    let mut hits = Vec::new();
    let q_lower = query.to_lowercase();
    walk(
        root,
        root,
        0,
        8,
        &q_lower,
        case_sensitive,
        query,
        &mut hits,
        max_results,
    );
    hits
}

/// Recursively search a local directory for `query`. Uses the bundled ripgrep
/// sidecar when present; falls back to system `rg`, then to a simple Rust
/// walker. Honors `.gitignore`, skips binary files, skips files >1 MB.
#[tauri::command]
pub async fn search_in_directory(
    app: tauri::AppHandle,
    root_path: String,
    query: String,
    case_sensitive: Option<bool>,
    use_regex: Option<bool>,
    max_results: Option<usize>,
) -> Result<SearchResult, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(SearchResult {
            hits: vec![],
            backend: "noop".into(),
        });
    }
    let root = std::path::Path::new(&root_path);
    if !root.is_dir() {
        return Err(format!("Not a directory: {}", root_path));
    }
    let case_sensitive = case_sensitive.unwrap_or(false);
    let use_regex = use_regex.unwrap_or(false);
    let max_results = max_results.unwrap_or(200).min(1000);

    let args = build_rg_args(&query, &root_path, case_sensitive, use_regex, max_results);

    // 1) Try bundled sidecar
    match run_rg_sidecar(&app, &args).await {
        Ok((stdout, _code)) => {
            let hits = parse_rg_json(&stdout, &root_path, max_results);
            return Ok(SearchResult {
                hits,
                backend: "ripgrep-sidecar".into(),
            });
        }
        Err(e) => {
            eprintln!("[operon-search] sidecar unavailable: {}", e);
        }
    }

    // 2) Try system PATH rg
    match run_rg_system(&args).await {
        Ok((stdout, _code)) => {
            let hits = parse_rg_json(&stdout, &root_path, max_results);
            return Ok(SearchResult {
                hits,
                backend: "ripgrep-system".into(),
            });
        }
        Err(e) => {
            eprintln!("[operon-search] system rg unavailable: {}", e);
        }
    }

    // 3) Rust walker fallback
    let hits = search_walker_fallback(root, &query, case_sensitive, max_results);
    Ok(SearchResult {
        hits,
        backend: "rust-walker".into(),
    })
}

// ---------------------------------------------------------------------------
//  Remote search — prefers `~/.operon/bin/rg` (installed by Operon), then
//  any `rg` on the user's PATH, then GNU `grep -rnHI` as a last resort.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct RemoteRgCapability {
    /// Absolute path to rg on the remote, if found. None → fall back to grep.
    pub rg_path: Option<String>,
    /// ripgrep version string ("14.1.1"), if rg_path is set.
    pub version: Option<String>,
    /// Whether this server has any grep at all (should always be true on HPC).
    pub has_grep: bool,
}

/// Probe the remote server for ripgrep. Checks `~/.operon/bin/rg` first
/// (Operon-installed), then `rg` on PATH, then `grep`.
#[tauri::command]
pub async fn check_remote_ripgrep(
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    profile_id: String,
) -> Result<RemoteRgCapability, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };
    // One probe script to check all three capabilities at once.
    let script = "\
        if [ -x \"$HOME/.operon/bin/rg\" ]; then \
            echo RG_PATH=$HOME/.operon/bin/rg; \
            \"$HOME/.operon/bin/rg\" --version 2>/dev/null | head -n1 || true; \
        elif command -v rg >/dev/null 2>&1; then \
            echo RG_PATH=$(command -v rg); \
            rg --version 2>/dev/null | head -n1 || true; \
        fi; \
        if command -v grep >/dev/null 2>&1; then echo HAS_GREP=1; fi";
    let output = crate::commands::ssh::ssh_exec(&profile, script)?;

    let mut rg_path = None;
    let mut version = None;
    let mut has_grep = false;
    for line in output.lines() {
        if let Some(p) = line.strip_prefix("RG_PATH=") {
            rg_path = Some(p.trim().to_string());
        } else if line == "HAS_GREP=1" {
            has_grep = true;
        } else if line.starts_with("ripgrep ") {
            // e.g. "ripgrep 14.1.1 ..."
            version = line.split_whitespace().nth(1).map(String::from);
        }
    }
    Ok(RemoteRgCapability {
        rg_path,
        version,
        has_grep,
    })
}

/// Install ripgrep on the remote server by scp'ing the bundled
/// musl-static Linux binary to `~/.operon/bin/rg`.
#[tauri::command]
pub async fn install_remote_ripgrep(
    app: tauri::AppHandle,
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    use tauri::Manager;
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Locate the bundled musl binary. In dev it's in src-tauri/binaries/;
    // in a production bundle it's copied to the app resource dir by the
    // "resources" entry in tauri.conf.json.
    let resource_path = app
        .path()
        .resolve(
            "binaries/rg-x86_64-unknown-linux-musl",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Cannot locate bundled ripgrep: {}", e))?;
    let local_path = if resource_path.exists() {
        resource_path
    } else {
        // Dev fallback — relative to cwd.
        let dev_path =
            std::path::PathBuf::from("./src-tauri/binaries/rg-x86_64-unknown-linux-musl");
        if dev_path.exists() {
            dev_path
        } else {
            return Err(format!(
                "Bundled ripgrep not found at {} or ./src-tauri/binaries/",
                resource_path.display()
            ));
        }
    };

    let local_path_str = local_path.to_string_lossy().to_string();
    let remote_tmp = "/tmp/operon-rg-upload";
    let remote_final = "~/.operon/bin/rg";

    // 1) Prepare remote dir
    crate::commands::ssh::ssh_exec(
        &profile,
        "mkdir -p $HOME/.operon/bin && rm -f /tmp/operon-rg-upload",
    )?;
    // 2) SCP upload — replicate scp_to_remote logic inline to avoid
    // going through the Tauri command layer.
    {
        let host_str = format!("{}@{}", profile.user, profile.host);
        let mut scp_args: Vec<String> = vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "ConnectTimeout=10".into(),
        ];
        if !crate::platform::supports_ssh_mux() {
            scp_args.push("-o".into());
            scp_args.push("PreferredAuthentications=publickey".into());
        }
        let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
        if sock.exists() {
            scp_args.push("-o".into());
            scp_args.push(format!("ControlPath={}", sock.to_string_lossy()));
        }
        if profile.port != 22 {
            scp_args.push("-P".into());
            scp_args.push(profile.port.to_string());
        }
        if let Some(key) = &profile.key_file {
            if std::path::Path::new(key).exists() {
                scp_args.push("-i".into());
                scp_args.push(key.clone());
            }
        }
        scp_args.push(local_path_str.clone());
        scp_args.push(format!("{}:{}", host_str, remote_tmp));

        let output = std::process::Command::new("scp")
            .args(&scp_args)
            .output()
            .map_err(|e| format!("Failed to run scp: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("SCP ripgrep upload failed: {}", stderr));
        }
    }
    // 3) Verify arch looks right and move into place
    let install_cmd = format!(
        "mv {tmp} $HOME/.operon/bin/rg && chmod +x $HOME/.operon/bin/rg && \
         $HOME/.operon/bin/rg --version 2>/dev/null | head -n1",
        tmp = remote_tmp,
    );
    let version_out = crate::commands::ssh::ssh_exec(&profile, &install_cmd)?;
    if !version_out.contains("ripgrep") {
        return Err(format!(
            "Install failed — binary does not run on this server. Output: {}",
            version_out.trim()
        ));
    }
    Ok(format!(
        "{} installed at {}",
        version_out.trim(),
        remote_final
    ))
}

/// Recursively search a remote directory. Uses remote rg if available,
/// falls back to `grep -rnHI`.
#[tauri::command]
pub async fn search_in_remote_directory(
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    profile_id: String,
    root_path: String,
    query: String,
    case_sensitive: Option<bool>,
    use_regex: Option<bool>,
    max_results: Option<usize>,
) -> Result<SearchResult, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(SearchResult {
            hits: vec![],
            backend: "noop".into(),
        });
    }
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };
    let case_sensitive = case_sensitive.unwrap_or(false);
    let use_regex = use_regex.unwrap_or(false);
    let max_results = max_results.unwrap_or(200).min(1000);

    let query_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        query.as_bytes(),
    );
    let root_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        root_path.as_bytes(),
    );

    // Probe once per call. Cheap; SSH mux keeps it fast.
    let probe = "\
        if [ -x \"$HOME/.operon/bin/rg\" ]; then echo $HOME/.operon/bin/rg; \
        elif command -v rg >/dev/null 2>&1; then echo rg; \
        else echo __no_rg__; fi";
    let rg_probe = crate::commands::ssh::ssh_exec(&profile, probe)?
        .trim()
        .to_string();
    let has_rg = rg_probe != "__no_rg__" && !rg_probe.is_empty();

    if has_rg {
        // Build a remote rg invocation mirroring the local flags.
        let mut rg_args: Vec<String> = Vec::new();
        rg_args.push("--json".into());
        rg_args.push("-n".into());
        rg_args.push("--max-count".into());
        rg_args.push(format!("{}", (max_results / 4).clamp(20, 500)));
        rg_args.push("--max-filesize".into());
        rg_args.push("1M".into());
        rg_args.push("--no-require-git".into());
        if case_sensitive {
            rg_args.push("--case-sensitive".into());
        } else {
            rg_args.push("--ignore-case".into());
        }
        if !use_regex {
            rg_args.push("--fixed-strings".into());
        }
        for d in SKIP_SEARCH_DIRS {
            rg_args.push("--glob".into());
            rg_args.push(format!("!**/{}/**", d));
        }
        // Pass query + root as the final args (we'll inject via $Q and $R to
        // avoid re-quoting every term).
        let rg_flags = rg_args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "Q=$(echo '{q}' | base64 -d); R=$(echo '{r}' | base64 -d); \
             {rg} {flags} -- \"$Q\" \"$R\" 2>/dev/null",
            q = query_b64,
            r = root_b64,
            rg = rg_probe,
            flags = rg_flags,
        );
        let script_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            script.as_bytes(),
        );
        let wrapped = format!("echo '{}' | base64 -d | bash", script_b64);
        let stdout = crate::commands::ssh::ssh_exec(&profile, &wrapped)?;
        let hits = parse_rg_json(stdout.as_bytes(), &root_path, max_results);
        return Ok(SearchResult {
            hits,
            backend: "ripgrep-remote".into(),
        });
    }

    // grep fallback — same shape as the pre-ripgrep implementation.
    let exclude_dirs = SKIP_SEARCH_DIRS
        .iter()
        .map(|d| format!("--exclude-dir='{}'", d))
        .collect::<Vec<_>>()
        .join(" ");
    let case_flag = if case_sensitive { "" } else { "-i " };
    let regex_flag = if use_regex { "-E " } else { "-F " };
    let remote_script = format!(
        "Q=$(echo '{q}' | base64 -d); R=$(echo '{r}' | base64 -d); \
cd \"$R\" 2>/dev/null || {{ echo \"__OPERON_ERR__: cannot cd $R\" >&2; exit 2; }}; \
grep -rnHI {case}{regex}{exc} -- \"$Q\" . 2>/dev/null | head -n {n}",
        q = query_b64,
        r = root_b64,
        case = case_flag,
        regex = regex_flag,
        exc = exclude_dirs,
        n = max_results,
    );
    let script_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        remote_script.as_bytes(),
    );
    let wrapped = format!("echo '{}' | base64 -d | bash", script_b64);
    let output = crate::commands::ssh::ssh_exec(&profile, &wrapped)?;

    let mut hits = Vec::new();
    for line in output.lines() {
        let line = line.strip_prefix("./").unwrap_or(line);
        let mut parts = line.splitn(3, ':');
        let path = match parts.next() {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => continue,
        };
        let lineno: u32 = match parts.next().and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => continue,
        };
        let text = parts.next().unwrap_or("").to_string();
        hits.push(SearchHit {
            path,
            line: lineno,
            text: truncate_match_text(&text),
        });
        if hits.len() >= max_results {
            break;
        }
    }
    Ok(SearchResult {
        hits,
        backend: "grep-remote".into(),
    })
}

// ---------------------------------------------------------------------------
//  Part C — regex-based "list files" for bulk @-mention in chat
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct RegexMatchResult {
    /// Relative paths (from root_path), capped at `max_results`.
    pub paths: Vec<String>,
    /// True number of matches found before capping.
    pub total_matched: usize,
    /// True if we stopped collecting because we hit `max_results`.
    pub truncated: bool,
}

const REGEX_HARD_CAP: usize = 5000;

fn regex_walk(
    base: &std::path::Path,
    dir: &std::path::Path,
    depth: usize,
    recursive: bool,
    re: &regex::Regex,
    match_full_path: bool,
    out: &mut Vec<String>,
    total: &mut usize,
    hard_cap: usize,
) {
    if *total >= hard_cap {
        return;
    }
    if !recursive && depth > 0 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        if *total >= hard_cap {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if crate::platform::is_hidden(&path) {
            continue;
        }
        let md = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.is_dir() {
            if SKIP_SEARCH_DIRS.contains(&name.as_str()) {
                continue;
            }
            if recursive {
                regex_walk(
                    base,
                    &path,
                    depth + 1,
                    recursive,
                    re,
                    match_full_path,
                    out,
                    total,
                    hard_cap,
                );
            }
            continue;
        }
        let rel = path
            .strip_prefix(base)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());
        let haystack = if match_full_path { &rel } else { &name };
        if re.is_match(haystack) {
            *total += 1;
            out.push(rel);
        }
    }
}

/// List files under `root_path` whose name (or relative path if
/// `match_full_path`) matches the user-supplied regex. Returns up to
/// `max_results` paths plus the total match count.
///
/// Regex flavor: RE2 (Rust `regex` crate) — no lookaround / backrefs.
#[tauri::command]
pub async fn list_files_matching_regex(
    root_path: String,
    pattern: String,
    recursive: Option<bool>,
    case_sensitive: Option<bool>,
    match_full_path: Option<bool>,
    max_results: Option<usize>,
) -> Result<RegexMatchResult, String> {
    let root = std::path::Path::new(&root_path);
    if !root.is_dir() {
        return Err(format!("Not a directory: {}", root_path));
    }
    let recursive = recursive.unwrap_or(true);
    let case_sensitive = case_sensitive.unwrap_or(false);
    let match_full_path = match_full_path.unwrap_or(false);
    let max_results = max_results.unwrap_or(1000).min(REGEX_HARD_CAP);

    let re = regex::RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut all: Vec<String> = Vec::new();
    let mut total = 0usize;
    regex_walk(
        root,
        root,
        0,
        recursive,
        &re,
        match_full_path,
        &mut all,
        &mut total,
        REGEX_HARD_CAP,
    );

    let truncated = all.len() > max_results;
    if truncated {
        all.truncate(max_results);
    }
    Ok(RegexMatchResult {
        paths: all,
        total_matched: total,
        truncated,
    })
}

/// Same as `list_files_matching_regex` but via SSH. Uses GNU `find` to list
/// and `grep -E` to filter. Regex flavor on the remote is ERE.
#[tauri::command]
pub async fn list_remote_files_matching_regex(
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    profile_id: String,
    root_path: String,
    pattern: String,
    recursive: Option<bool>,
    case_sensitive: Option<bool>,
    match_full_path: Option<bool>,
    max_results: Option<usize>,
) -> Result<RegexMatchResult, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };
    let recursive = recursive.unwrap_or(true);
    let case_sensitive = case_sensitive.unwrap_or(false);
    let match_full_path = match_full_path.unwrap_or(false);
    let max_results = max_results.unwrap_or(1000).min(REGEX_HARD_CAP);

    let root_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        root_path.as_bytes(),
    );
    let pattern_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        pattern.as_bytes(),
    );

    let depth_flag = if recursive { "" } else { "-maxdepth 1" };
    let case_flag = if case_sensitive { "" } else { "-i" };
    let skip_pruning = SKIP_SEARCH_DIRS
        .iter()
        .map(|d| format!("-name '{}' -prune -o", d))
        .collect::<Vec<_>>()
        .join(" ");

    // Strategy: find prints full absolute paths one per line. We then pipe
    // through `sed` to strip the root prefix and a leading `/`, then grep by
    // filename-only or full-relative depending on match_full_path.
    let match_expr = if match_full_path {
        // grep the whole relative path
        "cat"
    } else {
        // extract basename before grep — we split on the last slash inline
        "awk -F/ '{print $NF\"\\t\"$0}'"
    };

    let script = if match_full_path {
        format!(
            "R=$(echo '{r}' | base64 -d); P=$(echo '{p}' | base64 -d); \
find \"$R\" {depth} \\( {skip} -type f -print \\) 2>/dev/null \
  | sed -e \"s|^$R/||\" -e \"s|^$R||\" \
  | {match} \
  | grep -E {case} -- \"$P\" \
  | head -c 1048576",
            r = root_b64, p = pattern_b64,
            depth = depth_flag, skip = skip_pruning,
            match = match_expr,
            case = case_flag,
        )
    } else {
        // match only on basename — emit basename<TAB>relpath, grep the first
        // column, then strip the first column before returning.
        format!(
            "R=$(echo '{r}' | base64 -d); P=$(echo '{p}' | base64 -d); \
find \"$R\" {depth} \\( {skip} -type f -print \\) 2>/dev/null \
  | sed -e \"s|^$R/||\" -e \"s|^$R||\" \
  | awk -F/ '{{print $NF\"\\t\"$0}}' \
  | grep -E {case} -- \"$P\" \
  | awk -F'\\t' '{{print $2}}' \
  | head -c 1048576",
            r = root_b64, p = pattern_b64,
            depth = depth_flag, skip = skip_pruning,
            case = case_flag,
        )
    };

    let script_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        script.as_bytes(),
    );
    let wrapped = format!("echo '{}' | base64 -d | bash", script_b64);
    let output = crate::commands::ssh::ssh_exec(&profile, &wrapped)?;

    let mut all: Vec<String> = output
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let total = all.len();
    let truncated = total > max_results;
    if truncated {
        all.truncate(max_results);
    }
    Ok(RegexMatchResult {
        paths: all,
        total_matched: total,
        truncated,
    })
}
