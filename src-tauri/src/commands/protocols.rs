use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Serialize)]
pub struct ProtocolParam {
    pub name: String,
    pub default: String,
    pub kind: String,
    pub template_file: String,
}

fn user_protocols_dir() -> Option<PathBuf> {
    let dir = crate::platform::data_dir().join("protocols");
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// Reproduces the bundled-protocol search order used in `files::list_protocols` /
/// `files::read_protocol`: user dir → Tauri resource_dir → macOS Resources →
/// dev-mode walk up to find `src-tauri/protocols/` or `protocols/`.
fn protocol_search_dirs(app_handle: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(user) = user_protocols_dir() {
        dirs.push(user);
    }

    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        let bundled = resource_dir.join("protocols");
        if bundled.is_dir() {
            dirs.push(bundled);
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(resources) = exe_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("Resources").join("protocols"))
        {
            if resources.is_dir() {
                dirs.push(resources);
            }
        }
        let mut dir = exe_path.parent();
        for _ in 0..6 {
            if let Some(d) = dir {
                let src_tauri = d.join("src-tauri").join("protocols");
                if src_tauri.is_dir() {
                    dirs.push(src_tauri);
                    break;
                }
                let candidate = d.join("protocols");
                if candidate.is_dir() {
                    dirs.push(candidate);
                    break;
                }
                dir = d.parent();
            } else {
                break;
            }
        }
    }

    dirs
}

fn find_protocol_dir(app_handle: &tauri::AppHandle, slug: &str) -> Option<PathBuf> {
    for base in protocol_search_dirs(app_handle) {
        let candidate = base.join(slug);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn strip_quotes(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next().unwrap();
        let last = trimmed.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

/// Strip a `${VAR:-default}` wrapper down to just the default.
/// Examples: `${INPUT_BAM:-}` -> ``, `${N_THREADS:-1}` -> `1`,
/// `${OUTPUT_PREFIX:-scte_out}` -> `scte_out`.
fn unwrap_shell_default(raw: &str) -> String {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        if let Some((_var, default)) = inner.split_once(":-") {
            return default.to_string();
        }
        if let Some((_var, default)) = inner.split_once(":=") {
            return default.to_string();
        }
    }
    t.to_string()
}

fn infer_kind(name: &str, value: &str) -> String {
    let upper = name.to_uppercase();

    let path_suffixes = [
        "_DIR", "_FILE", "_BAM", "_BED", "_PATH", "_INDEX", "_VCF", "_FASTQ", "_FASTA", "_GTF",
        "_GFF", "_BIGWIG", "_BW", "_H5", "_H5AD",
    ];
    if path_suffixes.iter().any(|s| upper.ends_with(s))
        || upper.starts_with("INPUT")
        || upper.starts_with("OUTPUT")
    {
        return "path".to_string();
    }

    let lower_val = value.trim().to_ascii_lowercase();
    if matches!(lower_val.as_str(), "true" | "false" | "yes" | "no") {
        return "boolean".to_string();
    }

    let int_signal = upper.starts_with("N_")
        || upper.ends_with("_COUNT")
        || upper.ends_with("_THREADS")
        || upper.ends_with("_CORES")
        || upper.ends_with("_N");
    if int_signal {
        return "integer".to_string();
    }

    let float_signal = upper.starts_with("P_") || upper.starts_with("FC_");
    if float_signal {
        return "number".to_string();
    }

    if !value.trim().is_empty() {
        if value.trim().parse::<i64>().is_ok() {
            return "integer".to_string();
        }
        if value.trim().parse::<f64>().is_ok() {
            return "number".to_string();
        }
    }

    "string".to_string()
}

fn parse_configuration_block(content: &str, template_file: &str) -> Vec<ProtocolParam> {
    let mut params = Vec::new();
    let mut in_block = false;
    let mut saw_assignment_in_block = false;

    for raw_line in content.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if !in_block {
            // Look for any comment line containing "CONFIGURATION" but not
            // "END CONFIGURATION" — accommodates banner styles like
            // `# CONFIGURATION — edit these` and `# ===== CONFIGURATION =====`.
            if trimmed.starts_with('#')
                && trimmed.to_uppercase().contains("CONFIGURATION")
                && !trimmed.to_uppercase().contains("END CONFIGURATION")
            {
                in_block = true;
            }
            continue;
        }

        // Inside the block. Termination conditions:
        if trimmed.to_uppercase().contains("END CONFIGURATION") {
            break;
        }
        // Banner line of `====` characters after the block opener also ends it.
        if trimmed.starts_with('#')
            && trimmed.chars().filter(|c| *c == '=').count() >= 10
            && saw_assignment_in_block
        {
            break;
        }
        if trimmed.is_empty() {
            if saw_assignment_in_block {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') {
            // Comment inside the block — skip silently.
            continue;
        }

        // Expect `VAR=value` form.
        if let Some((lhs, rhs)) = trimmed.split_once('=') {
            let name = lhs.trim().to_string();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            // Strip trailing inline comment (but not from inside quoted strings).
            let rhs_clean = strip_inline_comment(rhs);
            let unwrapped = unwrap_shell_default(rhs_clean.trim());
            let default = strip_quotes(&unwrapped);
            let kind = infer_kind(&name, &default);
            params.push(ProtocolParam {
                name,
                default,
                kind,
                template_file: template_file.to_string(),
            });
            saw_assignment_in_block = true;
        }
    }

    params
}

fn strip_inline_comment(s: &str) -> String {
    let mut out = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for c in s.chars() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                out.push(c);
            }
            '"' if !in_single => {
                in_double = !in_double;
                out.push(c);
            }
            '#' if !in_single && !in_double => {
                break;
            }
            _ => out.push(c),
        }
    }
    out
}

fn collect_sh_templates(assets_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(assets_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("sh") {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

#[tauri::command]
pub async fn get_protocol_template_params(
    app_handle: tauri::AppHandle,
    slug: String,
) -> Result<Vec<ProtocolParam>, String> {
    let protocol_dir = find_protocol_dir(&app_handle, &slug)
        .ok_or_else(|| format!("Protocol '{}' not found", slug))?;

    let assets_dir = protocol_dir.join("assets");
    if !assets_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut all_params = Vec::new();
    for template_path in collect_sh_templates(&assets_dir) {
        let basename = template_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let Ok(content) = std::fs::read_to_string(&template_path) else {
            continue;
        };
        let mut params = parse_configuration_block(&content, &basename);
        all_params.append(&mut params);
    }

    Ok(all_params)
}
