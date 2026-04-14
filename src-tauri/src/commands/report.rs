use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Helper to suppress console windows on Windows for subprocess calls.
#[cfg(windows)]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000) // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

// ── Types ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScannedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_type: String, // "pdf", "image", "csv", "doc"
    pub columns: Option<Vec<String>>,
    pub rows: Option<u64>,
    pub dimensions: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanTreeNode {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub hint: Option<String>,
    pub files: Vec<ScannedFile>,
    pub children: Vec<ScanTreeNode>,
    pub total_file_count: u64,
    pub total_size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectScan {
    pub root: ScanTreeNode,
    pub total_pdfs: u64,
    pub total_images: u64,
    pub total_csvs: u64,
    pub total_docs: u64,
    pub total_code: u64,
    pub total_size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolEntry {
    pub name: String,
    pub version: String,
    pub language: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MethodsInfo {
    pub tools: Vec<ToolEntry>,
    pub r_version: Option<String>,
    pub python_version: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReportFigure {
    pub path: String,
    pub caption: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReportTable {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub caption: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReportCitation {
    pub index: u32,
    pub pmid: String,
    pub title: String,
    pub authors: String,
    pub journal: String,
    pub year: String,
    pub doi: Option<String>,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReportMethods {
    pub overview: String,
    pub tools: Vec<ToolEntry>,
    pub data_sources: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReportConfig {
    pub filename: String,
    pub output_dir: String,
    pub title: String,
    pub date: String,
    pub authors: Option<String>,
    pub abstract_text: String,
    pub introduction: Option<String>,
    pub results: String,
    pub discussion: String,
    pub methods: ReportMethods,
    pub figures: Vec<ReportFigure>,
    pub tables: Vec<ReportTable>,
    pub references: Vec<ReportCitation>,
}

// ── Constants ──

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "tiff", "tif",
];
const PDF_EXTS: &[&str] = &["pdf"];
const CSV_EXTS: &[&str] = &["csv", "tsv"];
const DOC_EXTS: &[&str] = &["md", "txt", "json", "html", "htm", "yaml", "yml", "toml"];
/// Source code and script files relevant for bioinformatics report context
const CODE_EXTS: &[&str] = &["r", "rmd", "py", "ipynb", "sh", "bash", "nf", "smk", "wdl"];

/// Max document file size for inclusion (2 MB)
const MAX_DOC_SIZE: u64 = 2 * 1024 * 1024;

/// Max CSV file size for inclusion (5 MB)
const MAX_CSV_SIZE: u64 = 5 * 1024 * 1024;
/// Max recursion depth for scanning
const MAX_SCAN_DEPTH: u32 = 8;

/// Directories to always skip
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    ".next",
    "__pycache__",
    ".cache",
    "target",
    "dist",
    "build",
    ".operon",
    ".vscode",
    ".idea",
    "venv",
    ".venv",
    "env",
    ".env",
    ".snakemake",
];

/// Directory name → heuristic hint mapping
fn dir_hint(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "results" | "output" | "outputs" | "final" | "analysis" => Some("results"),
        "plots" | "figures" | "images" | "fig" | "figs" | "figure" => Some("plots"),
        "raw" | "raw_data" | "rawdata" | "fastq" | "bam" | "cram" => Some("raw"),
        "tmp" | "temp" | "intermediate" | "scratch" | "bootstrap" | "logs" | "log" | "qc"
        | "qc_reports" => Some("intermediate"),
        "scripts" | "src" | "code" | "bin" | "slurm" | "jobs" => Some("scripts"),
        "reference" | "ref" | "genome" | "annotation" | "db" | "database" => Some("reference"),
        _ => None,
    }
}

// ── Commands ──

/// Scan a project directory for reportable files (PDFs, images, CSVs).
/// Returns a tree structure with heuristic hints for each directory.
#[tauri::command]
pub async fn scan_project_files(
    path: String,
    show_hidden: Option<bool>,
) -> Result<ProjectScan, String> {
    let root_path = PathBuf::from(&path);
    if !root_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let show_hidden = show_hidden.unwrap_or(false);
    let root = scan_dir_recursive(&root_path, &root_path, 0, show_hidden)?;

    // Count totals
    let (pdfs, images, csvs, docs, code, size) = count_totals(&root);

    Ok(ProjectScan {
        root,
        total_pdfs: pdfs,
        total_images: images,
        total_csvs: csvs,
        total_docs: docs,
        total_code: code,
        total_size: size,
    })
}

fn count_totals(node: &ScanTreeNode) -> (u64, u64, u64, u64, u64, u64) {
    let mut pdfs = 0u64;
    let mut images = 0u64;
    let mut csvs = 0u64;
    let mut docs = 0u64;
    let mut code = 0u64;
    let mut size = 0u64;

    for f in &node.files {
        size += f.size;
        match f.file_type.as_str() {
            "pdf" => pdfs += 1,
            "image" => images += 1,
            "csv" => csvs += 1,
            "doc" => docs += 1,
            "code" => code += 1,
            _ => {}
        }
    }
    for child in &node.children {
        let (p, i, c, d, co, s) = count_totals(child);
        pdfs += p;
        images += i;
        csvs += c;
        docs += d;
        code += co;
        size += s;
    }
    (pdfs, images, csvs, docs, code, size)
}

fn scan_dir_recursive(
    dir: &Path,
    _root: &Path,
    depth: u32,
    show_hidden: bool,
) -> Result<ScanTreeNode, String> {
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());

    let mut node = ScanTreeNode {
        path: dir.to_string_lossy().to_string(),
        name: dir_name.clone(),
        is_dir: true,
        hint: dir_hint(&dir_name).map(|s| s.to_string()),
        files: Vec::new(),
        children: Vec::new(),
        total_file_count: 0,
        total_size: 0,
    };

    if depth > MAX_SCAN_DEPTH {
        return Ok(node);
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory {}: {}", dir.display(), e))?;

    let mut dirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs unless requested
        if !show_hidden && name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Skip blacklisted directories
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            dirs.push(path);
        } else if path.is_file() {
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            // Skip log files regardless of extension
            let name_lower = name.to_lowercase();
            if name_lower.ends_with(".log")
                || name_lower.starts_with("log")
                || name_lower.contains(".log.")
            {
                continue;
            }

            let file_type = if PDF_EXTS.contains(&ext.as_str()) {
                Some("pdf")
            } else if IMAGE_EXTS.contains(&ext.as_str()) {
                Some("image")
            } else if CSV_EXTS.contains(&ext.as_str()) {
                Some("csv")
            } else if DOC_EXTS.contains(&ext.as_str()) {
                Some("doc")
            } else if CODE_EXTS.contains(&ext.as_str()) {
                Some("code")
            } else {
                None
            };

            if let Some(ft) = file_type {
                let metadata = std::fs::metadata(&path).ok();
                let size = metadata.map(|m| m.len()).unwrap_or(0);

                // Skip CSVs that are too large
                if ft == "csv" && size > MAX_CSV_SIZE {
                    continue;
                }
                // Skip docs that are too large
                if ft == "doc" && size > MAX_DOC_SIZE {
                    continue;
                }

                let mut scanned = ScannedFile {
                    path: path.to_string_lossy().to_string(),
                    name: name.clone(),
                    size,
                    file_type: ft.to_string(),
                    columns: None,
                    rows: None,
                    dimensions: None,
                };

                // For CSVs, extract basic metadata (column names, row count)
                if ft == "csv" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let mut lines = content.lines();
                        if let Some(header) = lines.next() {
                            let sep = if ext == "tsv" { '\t' } else { ',' };
                            let cols: Vec<String> = header
                                .split(sep)
                                .map(|s| s.trim().trim_matches('"').to_string())
                                .collect();
                            scanned.columns = Some(cols);
                        }
                        scanned.rows = Some(content.lines().count().saturating_sub(1) as u64);
                    }
                }

                node.files.push(scanned);
            }
        }
    }

    // Sort files by name
    node.files.sort_by(|a, b| a.name.cmp(&b.name));

    // Recurse into subdirectories
    dirs.sort();
    for d in dirs {
        let child = scan_dir_recursive(&d, _root, depth + 1, show_hidden)?;
        // Only include if it (or descendants) has reportable files
        if child.total_file_count > 0 || !child.files.is_empty() || !child.children.is_empty() {
            node.children.push(child);
        }
    }

    // Compute totals
    node.total_file_count = node.files.len() as u64
        + node
            .children
            .iter()
            .map(|c| c.total_file_count)
            .sum::<u64>();
    node.total_size = node.files.iter().map(|f| f.size).sum::<u64>()
        + node.children.iter().map(|c| c.total_size).sum::<u64>();

    Ok(node)
}

/// Scan project for tool/software versions used in the analysis.
/// Looks at common config files, scripts, and logs.
#[tauri::command]
pub async fn extract_methods_info(path: String) -> Result<MethodsInfo, String> {
    let root = PathBuf::from(&path);
    let mut tools: Vec<ToolEntry> = Vec::new();
    let mut evidence: Vec<String> = Vec::new();
    let mut r_version: Option<String> = None;
    let mut python_version: Option<String> = None;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Files to scan for version info
    let version_files = [
        // Python
        "requirements.txt",
        "setup.py",
        "setup.cfg",
        "pyproject.toml",
        // R
        "renv.lock",
        "DESCRIPTION",
        ".Rprofile",
        // Conda (parse for tool versions, not env details)
        "environment.yml",
        "environment.yaml",
        "conda_env.yml",
        // Nextflow / Snakemake
        "nextflow.config",
        "Snakefile",
        // Generic
        "Makefile",
        "Dockerfile",
    ];

    for filename in &version_files {
        let fpath = root.join(filename);
        if fpath.exists() {
            if let Ok(content) = std::fs::read_to_string(&fpath) {
                extract_versions_from_text(
                    &content,
                    filename,
                    &mut tools,
                    &mut evidence,
                    &mut seen,
                    &mut r_version,
                    &mut python_version,
                );
            }
        }
    }

    // Also scan R and Python scripts in the root and scripts/ directories
    let script_dirs = [".", "scripts", "src", "code", "R", "py"];
    for sd in &script_dirs {
        let dir = root.join(sd);
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if !p.is_file() {
                        continue;
                    }
                    let ext = p
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    if matches!(ext.as_str(), "r" | "rmd" | "py" | "sh" | "nf") {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            let fname = p
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            extract_versions_from_text(
                                &content,
                                &fname,
                                &mut tools,
                                &mut evidence,
                                &mut seen,
                                &mut r_version,
                                &mut python_version,
                            );
                        }
                    }
                }
            }
        }
    }

    // Sort tools alphabetically
    tools.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(MethodsInfo {
        tools,
        r_version,
        python_version,
        evidence,
    })
}

/// Regex-free version extraction from text content.
/// Looks for common patterns like `tool==version`, `library(tool)`, version strings, etc.
fn extract_versions_from_text(
    content: &str,
    source: &str,
    tools: &mut Vec<ToolEntry>,
    evidence: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    r_version: &mut Option<String>,
    python_version: &mut Option<String>,
) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') && !trimmed.contains("version") {
            continue;
        }

        // R version
        if trimmed.contains("R version") {
            if let Some(pos) = trimmed.find("R version") {
                let rest = &trimmed[pos + 10..];
                let ver: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if !ver.is_empty() && r_version.is_none() {
                    *r_version = Some(ver);
                    evidence.push(format!("[{}] {}", source, trimmed));
                }
            }
        }

        // Python version
        if trimmed.contains("python_requires")
            || trimmed.contains("python-version")
            || trimmed.contains("Python ")
        {
            let ver = extract_version_string(trimmed);
            if !ver.is_empty() && python_version.is_none() {
                *python_version = Some(ver);
                evidence.push(format!("[{}] {}", source, trimmed));
            }
        }

        // requirements.txt: package==version
        if source == "requirements.txt" || source.ends_with(".txt") {
            if let Some(idx) = trimmed.find("==") {
                let pkg = trimmed[..idx].trim();
                let ver = trimmed[idx + 2..].trim();
                if !pkg.is_empty() && !ver.is_empty() && !seen.contains(pkg) {
                    seen.insert(pkg.to_string());
                    tools.push(ToolEntry {
                        name: pkg.to_string(),
                        version: ver.to_string(),
                        language: Some("Python".to_string()),
                        category: None,
                    });
                }
            }
        }

        // renv.lock: "Package": "name", "Version": "x.y.z"
        if source == "renv.lock" {
            if trimmed.contains("\"Package\"") {
                // handled in pairs below
            }
            if trimmed.contains("\"Version\"") {
                let ver = extract_json_value(trimmed);
                // Try to get package name from evidence context
                if !ver.is_empty() {
                    if let Some(last) = evidence.last() {
                        if last.contains("\"Package\"") {
                            let pkg = extract_json_value(last.split("] ").last().unwrap_or(""));
                            if !pkg.is_empty() && !seen.contains(&pkg) {
                                seen.insert(pkg.clone());
                                tools.push(ToolEntry {
                                    name: pkg,
                                    version: ver,
                                    language: Some("R".to_string()),
                                    category: None,
                                });
                            }
                        }
                    }
                }
            }
            evidence.push(format!("[{}] {}", source, trimmed));
        }

        // environment.yml: - tool=version
        if (source.contains("environment") || source.contains("conda")) && trimmed.starts_with("- ")
        {
            let pkg_str = trimmed.trim_start_matches("- ").trim();
            if let Some(idx) = pkg_str.find('=') {
                let pkg = &pkg_str[..idx];
                let rest = &pkg_str[idx + 1..];
                let ver = rest.split('=').next().unwrap_or("").trim();
                if !pkg.is_empty() && !ver.is_empty() && !seen.contains(pkg) {
                    // Skip conda/pip infrastructure entries
                    if !matches!(pkg, "python" | "pip" | "conda" | "setuptools" | "wheel") {
                        seen.insert(pkg.to_string());
                        tools.push(ToolEntry {
                            name: pkg.to_string(),
                            version: ver.to_string(),
                            language: None,
                            category: None,
                        });
                    }
                    // But do capture python version
                    if pkg == "python" && python_version.is_none() {
                        *python_version = Some(ver.to_string());
                    }
                }
            }
        }

        // library() calls in R scripts
        if trimmed.contains("library(") || trimmed.contains("require(") {
            let start = if trimmed.contains("library(") {
                "library("
            } else {
                "require("
            };
            if let Some(pos) = trimmed.find(start) {
                let rest = &trimmed[pos + start.len()..];
                if let Some(end) = rest.find(')') {
                    let pkg = rest[..end].trim().trim_matches('"').trim_matches('\'');
                    if !pkg.is_empty() && !seen.contains(pkg) {
                        seen.insert(pkg.to_string());
                        tools.push(ToolEntry {
                            name: pkg.to_string(),
                            version: "".to_string(), // version unknown from library() call
                            language: Some("R".to_string()),
                            category: None,
                        });
                    }
                }
            }
        }

        // import statements in Python
        if (trimmed.starts_with("import ") || trimmed.starts_with("from "))
            && (source.ends_with(".py") || source.ends_with(".nf"))
        {
            let pkg = if trimmed.starts_with("from ") {
                trimmed
                    .strip_prefix("from ")
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("")
            } else {
                trimmed
                    .strip_prefix("import ")
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
            };
            // Skip standard library modules
            let stdlib = [
                "os",
                "sys",
                "re",
                "json",
                "csv",
                "math",
                "glob",
                "shutil",
                "pathlib",
                "subprocess",
                "argparse",
                "logging",
                "collections",
                "itertools",
                "functools",
                "typing",
                "datetime",
                "io",
                "copy",
                "warnings",
                "time",
                "hashlib",
                "tempfile",
                "textwrap",
                "string",
            ];
            if !pkg.is_empty() && !seen.contains(pkg) && !stdlib.contains(&pkg) {
                seen.insert(pkg.to_string());
                tools.push(ToolEntry {
                    name: pkg.to_string(),
                    version: "".to_string(),
                    language: Some("Python".to_string()),
                    category: None,
                });
            }
        }
    }
}

fn extract_version_string(s: &str) -> String {
    let mut result = String::new();
    let mut found_digit = false;
    for ch in s.chars() {
        if ch.is_ascii_digit() || (ch == '.' && found_digit) {
            result.push(ch);
            found_digit = true;
        } else if found_digit {
            break;
        }
    }
    result
}

fn extract_json_value(s: &str) -> String {
    // Extract value from "Key": "value"
    if let Some(colon_pos) = s.find(':') {
        let rest = s[colon_pos + 1..]
            .trim()
            .trim_matches('"')
            .trim_matches(',')
            .trim_matches('"')
            .trim();
        return rest.to_string();
    }
    String::new()
}

/// Read a CSV file and return headers + first N rows for preview/table inclusion.
#[tauri::command]
pub async fn read_csv_for_report(
    path: String,
    max_rows: Option<u32>,
) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let max = max_rows.unwrap_or(50) as usize;
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Cannot read CSV {}: {}", path, e))?;

    let ext = Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let sep = if ext == "tsv" { '\t' } else { ',' };

    let mut lines = content.lines();
    let headers: Vec<String> = lines
        .next()
        .map(|h| {
            h.split(sep)
                .map(|s| s.trim().trim_matches('"').to_string())
                .collect()
        })
        .unwrap_or_default();

    let rows: Vec<Vec<String>> = lines
        .take(max)
        .map(|line| {
            line.split(sep)
                .map(|s| s.trim().trim_matches('"').to_string())
                .collect()
        })
        .collect();

    Ok((headers, rows))
}

/// Generate a report PDF using a Python subprocess with reportlab.
/// Takes a JSON-serialized ReportConfig, writes the PDF, returns the output path.
#[tauri::command]
pub async fn generate_report_pdf(config: ReportConfig) -> Result<String, String> {
    let output_path = PathBuf::from(&config.output_dir).join(&config.filename);

    // Serialize config to JSON
    let config_json = serde_json::to_string(&config)
        .map_err(|e| format!("Failed to serialize report config: {}", e))?;

    // Write config to a temp file
    let tmp_dir = std::env::temp_dir();
    let config_file = tmp_dir.join("operon_report_config.json");
    std::fs::write(&config_file, &config_json)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    // Find the report generator script
    // It ships alongside the binary in resources/
    let script_content = include_str!("../../resources/report_generator.py");
    let script_file = tmp_dir.join("operon_report_generator.py");
    std::fs::write(&script_file, script_content)
        .map_err(|e| format!("Failed to write generator script: {}", e))?;

    // Run the Python generator
    let python = crate::platform::python_command();
    let output = hide_window(std::process::Command::new(python).args([
        script_file.to_string_lossy().as_ref(),
        config_file.to_string_lossy().as_ref(),
        output_path.to_string_lossy().as_ref(),
    ]))
    .output()
    .map_err(|e| format!("Failed to run report generator: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Auto-install reportlab if that's what's missing.
        // Try multiple strategies: --user (macOS Homebrew), --break-system-packages (Linux), venv fallback.
        if stderr.contains("No module named 'reportlab'") || stderr.contains("ModuleNotFoundError")
        {
            eprintln!("[operon] reportlab not found, attempting auto-install...");

            let install_strategies: Vec<Vec<&str>> = vec![
                // Strategy 1: --user install (works on macOS Homebrew Python)
                vec!["-m", "pip", "install", "reportlab", "--user", "--quiet"],
                // Strategy 2: --break-system-packages (works on some Linux distros)
                vec![
                    "-m",
                    "pip",
                    "install",
                    "reportlab",
                    "--quiet",
                    "--break-system-packages",
                ],
                // Strategy 3: pip3 directly with --user
                // (handled below as a separate command if python3 -m pip fails)
            ];

            let mut installed = false;
            for strategy in &install_strategies {
                eprintln!("[operon] Trying: {} {}", python, strategy.join(" "));
                let install =
                    hide_window(std::process::Command::new(python).args(strategy)).output();
                if let Ok(install_out) = install {
                    if install_out.status.success() {
                        installed = true;
                        eprintln!("[operon] reportlab installed successfully");
                        break;
                    }
                    let install_err = String::from_utf8_lossy(&install_out.stderr);
                    eprintln!(
                        "[operon] Install strategy failed: {}",
                        install_err.chars().take(200).collect::<String>()
                    );
                }
            }

            // Fallback: try pip/pip3 directly (some systems have pip3 but not python3 -m pip)
            if !installed {
                let pip_cmd = if cfg!(target_os = "windows") {
                    "pip"
                } else {
                    "pip3"
                };
                eprintln!(
                    "[operon] Trying: {} install reportlab --user --quiet",
                    pip_cmd
                );
                if let Ok(pip_out) = hide_window(std::process::Command::new(pip_cmd).args([
                    "install",
                    "reportlab",
                    "--user",
                    "--quiet",
                ]))
                .output()
                {
                    if pip_out.status.success() {
                        installed = true;
                        eprintln!("[operon] reportlab installed via {}", pip_cmd);
                    }
                }
            }

            if installed {
                // Retry PDF generation after installing
                let retry = hide_window(std::process::Command::new(python).args([
                    script_file.to_string_lossy().as_ref(),
                    config_file.to_string_lossy().as_ref(),
                    output_path.to_string_lossy().as_ref(),
                ]))
                .output()
                .map_err(|e| format!("Failed to retry report generator: {}", e))?;

                if retry.status.success() {
                    let _ = std::fs::remove_file(&config_file);
                    let _ = std::fs::remove_file(&script_file);
                    return Ok(output_path.to_string_lossy().to_string());
                }
                let retry_stderr = String::from_utf8_lossy(&retry.stderr);
                return Err(format!(
                    "Report generation failed after installing reportlab: {}",
                    retry_stderr
                ));
            }
        }

        return Err(format!("Report generation failed: {}", stderr));
    }

    // Clean up temp files
    let _ = std::fs::remove_file(&config_file);
    let _ = std::fs::remove_file(&script_file);

    Ok(output_path.to_string_lossy().to_string())
}

/// Generate report for a remote project over SSH.
#[tauri::command]
pub async fn scan_remote_project_files(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
    path: String,
) -> Result<ProjectScan, String> {
    // Use SSH to list files and build the scan tree
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or(format!("SSH profile '{}' not found", profile_id))?
            .clone()
    };

    // Use find command to get all reportable files with sizes.
    // Must match all extensions from PDF_EXTS, IMAGE_EXTS, CSV_EXTS, and DOC_EXTS.
    //
    // Strategy: Try GNU find -printf first (most Linux HPC systems have this).
    // The shell_escape() wrapper single-quotes the entire command, so we can't use
    // shell variables ($f, $s). Keep it to a single find invocation.
    let escaped_path = path.replace('\'', "'\\''");
    let find_cmd = format!(
        "find '{p}' -maxdepth {d} -type f \\( \
         -name '*.pdf' \
         -o -name '*.png' -o -name '*.jpg' -o -name '*.jpeg' -o -name '*.gif' \
         -o -name '*.bmp' -o -name '*.webp' -o -name '*.svg' -o -name '*.tiff' -o -name '*.tif' \
         -o -name '*.csv' -o -name '*.tsv' \
         -o -name '*.md' -o -name '*.txt' -o -name '*.json' -o -name '*.html' -o -name '*.htm' \
         -o -name '*.yaml' -o -name '*.yml' -o -name '*.toml' \
         -o -name '*.R' -o -name '*.r' -o -name '*.Rmd' -o -name '*.rmd' \
         -o -name '*.py' -o -name '*.ipynb' -o -name '*.sh' -o -name '*.bash' \
         -o -name '*.nf' -o -name '*.smk' -o -name '*.wdl' \
         \\) \
         -not -path '*/.git/*' -not -path '*/node_modules/*' -not -path '*/__pycache__/*' \
         -not -path '*/.operon/*' -not -path '*/.cache/*' -not -path '*/target/*' \
         -not -path '*/.snakemake/*' -not -path '*/.nextflow/*' \
         -printf '%s\\t%p\\n' 2>/dev/null | head -5000",
        p = escaped_path,
        d = MAX_SCAN_DEPTH
    );

    eprintln!(
        "[operon] Remote scan command (first 300 chars): {}",
        &find_cmd[..find_cmd.len().min(300)]
    );
    let output =
        super::ssh::ssh_exec(&profile, &find_cmd).map_err(|e| format!("SSH scan failed: {}", e))?;
    eprintln!(
        "[operon] Remote scan output: {} bytes, {} lines",
        output.len(),
        output.lines().count()
    );

    // If -printf produced no output, fall back to find without sizes (use 0 for all sizes).
    // This handles systems where GNU find -printf is not available.
    let output = if output.trim().is_empty() {
        eprintln!("[operon] -printf produced no output, falling back to plain find");
        let fallback_cmd = format!(
            "find '{p}' -maxdepth {d} -type f \\( \
             -name '*.pdf' \
             -o -name '*.png' -o -name '*.jpg' -o -name '*.jpeg' -o -name '*.gif' \
             -o -name '*.bmp' -o -name '*.webp' -o -name '*.svg' -o -name '*.tiff' -o -name '*.tif' \
             -o -name '*.csv' -o -name '*.tsv' \
             -o -name '*.md' -o -name '*.txt' -o -name '*.json' -o -name '*.html' -o -name '*.htm' \
             -o -name '*.yaml' -o -name '*.yml' -o -name '*.toml' \
             \\) \
             -not -path '*/.git/*' -not -path '*/node_modules/*' -not -path '*/__pycache__/*' \
             -not -path '*/.operon/*' -not -path '*/.cache/*' -not -path '*/target/*' \
             -not -path '*/.snakemake/*' -not -path '*/.nextflow/*' \
             2>/dev/null | head -5000",
            p = escaped_path,
            d = MAX_SCAN_DEPTH
        );
        let fallback_output = super::ssh::ssh_exec(&profile, &fallback_cmd)
            .map_err(|e| format!("SSH scan fallback failed: {}", e))?;
        eprintln!(
            "[operon] Fallback scan output: {} bytes, {} lines",
            fallback_output.len(),
            fallback_output.lines().count()
        );
        // Convert plain paths to tab-delimited format with size=0
        fallback_output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| format!("0\t{}", l))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        output
    };

    // Parse find output into a flat list, then build tree
    let mut files_by_dir: HashMap<String, Vec<ScannedFile>> = HashMap::new();
    let _root_p = PathBuf::from(&path);

    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() != 2 {
            continue;
        }
        let size: u64 = parts[0].parse().unwrap_or(0);
        let file_path = parts[1];
        let fp = PathBuf::from(file_path);
        let name = fp
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let dir = fp
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = fp
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Skip log files
        let name_lower = name.to_lowercase();
        if name_lower.ends_with(".log")
            || name_lower.starts_with("log")
            || name_lower.contains(".log.")
        {
            continue;
        }

        let file_type = if PDF_EXTS.contains(&ext.as_str()) {
            "pdf"
        } else if IMAGE_EXTS.contains(&ext.as_str()) {
            "image"
        } else if CSV_EXTS.contains(&ext.as_str()) {
            if size > MAX_CSV_SIZE {
                continue;
            }
            "csv"
        } else if DOC_EXTS.contains(&ext.as_str()) {
            if size > MAX_DOC_SIZE {
                continue;
            }
            "doc"
        } else if CODE_EXTS.contains(&ext.as_str()) {
            if size > MAX_DOC_SIZE {
                continue;
            }
            "code"
        } else {
            continue;
        };

        files_by_dir.entry(dir).or_default().push(ScannedFile {
            path: file_path.to_string(),
            name,
            size,
            file_type: file_type.to_string(),
            columns: None,
            rows: None,
            dimensions: None,
        });
    }

    // Build tree from flat map
    let root = build_tree_from_flat(&path, &files_by_dir);
    let (pdfs, images, csvs, docs, code, total_size) = count_totals(&root);

    Ok(ProjectScan {
        root,
        total_pdfs: pdfs,
        total_images: images,
        total_csvs: csvs,
        total_docs: docs,
        total_code: code,
        total_size,
    })
}

fn build_tree_from_flat(
    root_path: &str,
    files_by_dir: &HashMap<String, Vec<ScannedFile>>,
) -> ScanTreeNode {
    let root_name = PathBuf::from(root_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_path.to_string());

    let mut root = ScanTreeNode {
        path: root_path.to_string(),
        name: root_name,
        is_dir: true,
        hint: None,
        files: files_by_dir.get(root_path).cloned().unwrap_or_default(),
        children: Vec::new(),
        total_file_count: 0,
        total_size: 0,
    };

    // Collect all unique directory paths that are under root
    let mut all_dirs: Vec<String> = files_by_dir
        .keys()
        .filter(|d| d.as_str() != root_path && d.starts_with(root_path))
        .cloned()
        .collect();
    all_dirs.sort();

    // Build a proper nested tree by recursively grouping under immediate children.
    // Group all dirs by their immediate child directory name relative to root_path.
    let mut child_dirs_map: HashMap<String, Vec<String>> = HashMap::new();

    for dir_path in &all_dirs {
        let relative = dir_path
            .strip_prefix(root_path)
            .unwrap_or(dir_path)
            .trim_start_matches('/');
        let first_part = relative.split('/').next().unwrap_or(relative);
        child_dirs_map
            .entry(first_part.to_string())
            .or_default()
            .push(dir_path.clone());
    }

    // For each immediate child directory, recursively build a subtree
    for child_name in child_dirs_map.keys() {
        let child_path = format!("{}/{}", root_path, child_name);
        // Recursively build the subtree rooted at child_path
        let child_node = build_tree_from_flat(&child_path, files_by_dir);
        // Apply directory hint based on the child name
        let child_with_hint = ScanTreeNode {
            hint: dir_hint(child_name)
                .map(|s| s.to_string())
                .or(child_node.hint),
            ..child_node
        };
        root.children.push(child_with_hint);
    }

    root.children.sort_by(|a, b| a.name.cmp(&b.name));

    // Update root totals from files and children
    fn sum_files(node: &ScanTreeNode) -> (u64, u64) {
        let own_count = node.files.len() as u64;
        let own_size: u64 = node.files.iter().map(|f| f.size).sum();
        let (child_count, child_size): (u64, u64) = node
            .children
            .iter()
            .map(sum_files)
            .fold((0, 0), |(ac, as_), (cc, cs)| (ac + cc, as_ + cs));
        (own_count + child_count, own_size + child_size)
    }
    let (total_count, total_sz) = sum_files(&root);
    root.total_file_count = total_count;
    root.total_size = total_sz;

    root
}

// ── File preview reading ──

/// Maximum bytes to read per file for report context
const MAX_PREVIEW_BYTES: usize = 8 * 1024; // 8 KB per file
/// Maximum lines to include per file
const MAX_PREVIEW_LINES: usize = 150;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilePreview {
    pub path: String,
    pub name: String,
    pub content: String,
    pub truncated: bool,
    pub error: Option<String>,
}

/// Read previews of multiple local files for report context.
/// Returns truncated text content for text-based files (CSV, TSV, MD, TXT, JSON, R, PY, etc.).
/// Skips binary files (PDF, images).
#[tauri::command]
pub async fn batch_read_file_previews(paths: Vec<String>) -> Result<Vec<FilePreview>, String> {
    let text_exts: &[&str] = &[
        "csv", "tsv", "md", "txt", "json", "html", "htm", "yaml", "yml", "toml", "r", "rmd", "py",
        "ipynb", "sh", "bash", "nf", "smk", "wdl",
    ];

    let mut results = Vec::new();
    for file_path in &paths {
        let fp = PathBuf::from(file_path);
        let name = fp
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone());
        let ext = fp
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Skip binary files — their paths are still listed but content not read
        if !text_exts.contains(&ext.as_str()) {
            results.push(FilePreview {
                path: file_path.clone(),
                name,
                content: format!("[Binary file: {}]", ext.to_uppercase()),
                truncated: false,
                error: None,
            });
            continue;
        }

        match std::fs::File::open(&fp) {
            Ok(mut f) => {
                let mut buf = vec![0u8; MAX_PREVIEW_BYTES];
                let bytes_read = f.read(&mut buf).unwrap_or(0);
                buf.truncate(bytes_read);
                let text = String::from_utf8_lossy(&buf).to_string();

                // Truncate to MAX_PREVIEW_LINES
                let lines: Vec<&str> = text.lines().collect();
                let truncated = lines.len() > MAX_PREVIEW_LINES || bytes_read == MAX_PREVIEW_BYTES;
                let preview = if lines.len() > MAX_PREVIEW_LINES {
                    lines[..MAX_PREVIEW_LINES].join("\n")
                } else {
                    text
                };

                results.push(FilePreview {
                    path: file_path.clone(),
                    name,
                    content: preview,
                    truncated,
                    error: None,
                });
            }
            Err(e) => {
                results.push(FilePreview {
                    path: file_path.clone(),
                    name,
                    content: String::new(),
                    truncated: false,
                    error: Some(format!("Cannot read: {}", e)),
                });
            }
        }
    }
    Ok(results)
}

/// Read previews of multiple remote files over SSH for report context.
/// Batches reads into a single SSH command for efficiency.
#[tauri::command]
pub async fn batch_read_remote_file_previews(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
    paths: Vec<String>,
) -> Result<Vec<FilePreview>, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or(format!("SSH profile '{}' not found", profile_id))?
            .clone()
    };

    let text_exts: &[&str] = &[
        "csv", "tsv", "md", "txt", "json", "html", "htm", "yaml", "yml", "toml", "r", "rmd", "py",
        "ipynb", "sh", "bash", "nf", "smk", "wdl",
    ];

    let mut results = Vec::new();

    for file_path in &paths {
        let fp = PathBuf::from(file_path);
        let name = fp
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone());
        let ext = fp
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Skip binary files
        if !text_exts.contains(&ext.as_str()) {
            results.push(FilePreview {
                path: file_path.clone(),
                name,
                content: format!("[Binary file: {}]", ext.to_uppercase()),
                truncated: false,
                error: None,
            });
            continue;
        }

        // Read file via SSH: head -N to limit lines, then truncate bytes
        let escaped = file_path.replace('\'', "'\\''");
        let cmd = format!(
            "head -n {} '{}' 2>/dev/null | head -c {}",
            MAX_PREVIEW_LINES, escaped, MAX_PREVIEW_BYTES
        );
        match super::ssh::ssh_exec(&profile, &cmd) {
            Ok(output) => {
                let truncated = output.lines().count() >= MAX_PREVIEW_LINES
                    || output.len() >= MAX_PREVIEW_BYTES;
                results.push(FilePreview {
                    path: file_path.clone(),
                    name,
                    content: output,
                    truncated,
                    error: None,
                });
            }
            Err(e) => {
                results.push(FilePreview {
                    path: file_path.clone(),
                    name,
                    content: String::new(),
                    truncated: false,
                    error: Some(format!("SSH read failed: {}", e)),
                });
            }
        }
    }

    Ok(results)
}
