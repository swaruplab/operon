use serde::{Deserialize, Serialize};
use tauri::{Manager, Emitter};

/// Run a shell command in a specific directory, return stdout or error
fn run_in_dir(command: &str, dir: &str) -> Result<String, String> {
    let escaped_dir = dir.replace('\'', "'\\''");
    let full_cmd = format!("cd '{}' && {}", escaped_dir, command);
    let output = crate::platform::shell_exec(&full_cmd)
        .output()
        .map_err(|e| format!("Failed to run command: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

// ──────────────────────────────────────────────
// Status & Info
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: String,
    pub changed_files: u32,
    pub staged_files: u32,
    pub untracked_files: u32,
    pub ahead: u32,
    pub behind: u32,
    pub remote_url: String,
    pub has_remote: bool,
    pub last_commit_message: String,
    pub last_commit_time: String,
}

#[tauri::command]
pub async fn git_status(project_path: String) -> Result<GitStatus, String> {
    // Check if it's a git repo
    let is_repo = run_in_dir("git rev-parse --is-inside-work-tree", &project_path)
        .map(|o| o == "true")
        .unwrap_or(false);

    if !is_repo {
        return Ok(GitStatus {
            is_repo: false,
            branch: String::new(),
            changed_files: 0,
            staged_files: 0,
            untracked_files: 0,
            ahead: 0,
            behind: 0,
            remote_url: String::new(),
            has_remote: false,
            last_commit_message: String::new(),
            last_commit_time: String::new(),
        });
    }

    let branch = run_in_dir("git branch --show-current", &project_path)
        .unwrap_or_default();

    // Count changed, staged, untracked
    let status_output = run_in_dir("git status --porcelain", &project_path)
        .unwrap_or_default();

    let mut changed: u32 = 0;
    let mut staged: u32 = 0;
    let mut untracked: u32 = 0;

    for line in status_output.lines() {
        if line.len() < 2 { continue; }
        let xy: Vec<char> = line.chars().take(2).collect();
        if xy[0] == '?' { untracked += 1; }
        else {
            if xy[0] != ' ' && xy[0] != '?' { staged += 1; }
            if xy[1] != ' ' && xy[1] != '?' { changed += 1; }
        }
    }

    // Ahead/behind
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    if let Ok(ab) = run_in_dir("git rev-list --left-right --count HEAD...@{upstream} 2>/dev/null", &project_path) {
        let parts: Vec<&str> = ab.split_whitespace().collect();
        if parts.len() == 2 {
            ahead = parts[0].parse().unwrap_or(0);
            behind = parts[1].parse().unwrap_or(0);
        }
    }

    // Remote URL
    let remote_url = run_in_dir("git remote get-url origin 2>/dev/null", &project_path)
        .unwrap_or_default();
    let has_remote = !remote_url.is_empty();

    // Last commit
    let last_commit_message = run_in_dir("git log -1 --format=%s 2>/dev/null", &project_path)
        .unwrap_or_default();
    let last_commit_time = run_in_dir("git log -1 --format=%ar 2>/dev/null", &project_path)
        .unwrap_or_default();

    Ok(GitStatus {
        is_repo,
        branch,
        changed_files: changed,
        staged_files: staged,
        untracked_files: untracked,
        ahead,
        behind,
        remote_url,
        has_remote,
        last_commit_message,
        last_commit_time,
    })
}

// ──────────────────────────────────────────────
// Git Operations
// ──────────────────────────────────────────────

#[tauri::command]
pub async fn git_init(project_path: String) -> Result<(), String> {
    run_in_dir("git init", &project_path)?;
    Ok(())
}

#[tauri::command]
pub async fn git_commit_all(project_path: String, message: String) -> Result<String, String> {
    // Stage everything
    run_in_dir("git add -A", &project_path)?;

    // Commit
    let safe_message = message.replace('\'', "'\\''");
    let result = run_in_dir(&format!("git commit -m '{}'", safe_message), &project_path)?;
    Ok(result)
}

#[tauri::command]
pub async fn git_push(project_path: String) -> Result<String, String> {
    // Try regular push first
    match run_in_dir("git push", &project_path) {
        Ok(r) => Ok(r),
        Err(_) => {
            // If no upstream, set it
            let branch = run_in_dir("git branch --show-current", &project_path)
                .unwrap_or_else(|_| "main".to_string());
            run_in_dir(&format!("git push -u origin {}", branch), &project_path)
        }
    }
}

// ──────────────────────────────────────────────
// GitHub CLI Integration
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GhAuthStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub username: String,
    pub scopes: String,
}

#[tauri::command]
pub async fn gh_check_auth() -> Result<GhAuthStatus, String> {
    // Check if gh is installed
    let installed = crate::platform::shell_exec("which gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !installed {
        return Ok(GhAuthStatus {
            installed: false,
            authenticated: false,
            username: String::new(),
            scopes: String::new(),
        });
    }

    // Check auth status
    let auth_output = crate::platform::shell_exec("gh auth status 2>&1")
        .output()
        .map_err(|e| e.to_string())?;

    let output_text = String::from_utf8_lossy(&auth_output.stdout).to_string()
        + &String::from_utf8_lossy(&auth_output.stderr).to_string();

    let authenticated = auth_output.status.success() || output_text.contains("Logged in to");

    // Get username
    let username = if authenticated {
        crate::platform::shell_exec("gh api user --jq .login 2>/dev/null")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    Ok(GhAuthStatus {
        installed,
        authenticated,
        username,
        scopes: String::new(),
    })
}

#[tauri::command]
pub async fn gh_install() -> Result<(), String> {
    // Try platform package manager first
    if let Some(pkg_mgr) = crate::platform::find_package_manager() {
        let cmd = if pkg_mgr.contains("brew") {
            format!("{} install gh", pkg_mgr)
        } else if pkg_mgr.contains("winget") {
            format!("{} install --id GitHub.cli --accept-source-agreements --accept-package-agreements", pkg_mgr)
        } else {
            // apt
            "sudo apt-get install -y gh".to_string()
        };
        let tmp = crate::platform::temp_dir().to_string_lossy().to_string();
        run_in_dir(&cmd, &tmp)?;
        Ok(())
    } else {
        Err("No package manager found. Please install GitHub CLI manually: https://cli.github.com".to_string())
    }
}

/// Step 1: Start gh login, capture the one-time code + open browser. Returns the code.
#[tauri::command]
pub async fn gh_login(app_handle: tauri::AppHandle) -> Result<String, String> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    // Check if already logged in
    let check = crate::platform::shell_exec("gh auth status 2>&1")
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).to_string()
                + &String::from_utf8_lossy(&o.stderr).to_string()
        })
        .unwrap_or_default();
    if check.contains("Logged in") {
        return Ok("ALREADY_AUTHED".to_string());
    }

    let mut child = crate::platform::shell_exec("gh auth login --hostname github.com --git-protocol https --scopes repo,read:org --web 2>&1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to launch GitHub login: {}", e))?;

    let stdout = child.stdout.take();
    let app = app_handle.clone();

    // Read output on a background thread — extract the code and emit it to the frontend
    std::thread::spawn(move || {
        if let Some(out) = stdout {
            let reader = BufReader::new(out);
            for line in reader.lines() {
                if let Ok(line) = line {
                    // gh prints something like: "! First copy your one-time code: ABCD-1234"
                    if line.contains("one-time code:") {
                        if let Some(code) = line.split("one-time code:").nth(1) {
                            let one_time_code = code.trim().to_string();
                            // Emit the code to the frontend so it can be displayed
                            if let Some(window) = app.get_webview_window("main") {
                                let _: Result<(), _> = window.emit("gh-login-code", &one_time_code);
                            }
                        }
                    }
                    // When gh says to open URL, open it
                    if line.contains("https://github.com/login/device") {
                        let _ = crate::platform::open_url("https://github.com/login/device");
                    }
                }
            }
        }

        // Wait for process to complete
        let _ = child.wait();

        // Notify frontend that login completed
        if let Some(window) = app.get_webview_window("main") {
            // Check final auth status
            let ok = crate::platform::shell_exec("gh auth status 2>&1")
                .output()
                .map(|o| {
                    let out = String::from_utf8_lossy(&o.stdout).to_string()
                        + &String::from_utf8_lossy(&o.stderr).to_string();
                    out.contains("Logged in")
                })
                .unwrap_or(false);
            let _: Result<(), _> = window.emit("gh-login-done", ok);
        }
    });

    // Return immediately — the frontend will listen for events
    Ok("LOGIN_STARTED".to_string())
}

#[tauri::command]
pub async fn gh_create_repo(
    project_path: String,
    repo_name: String,
    private: bool,
    description: String,
) -> Result<String, String> {
    let visibility = if private { "--private" } else { "--public" };
    let desc_flag = if description.is_empty() {
        String::new()
    } else {
        format!("--description '{}'", description.replace('\'', "'\\''"))
    };

    // Create repo and set it as remote
    let cmd = format!(
        "gh repo create '{}' {} {} --source='.' --remote=origin --push",
        repo_name.replace('\'', "'\\''"),
        visibility,
        desc_flag,
    );

    let result = run_in_dir(&cmd, &project_path)?;
    Ok(result)
}

// ──────────────────────────────────────────────
// Auto-versioning
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionInfo {
    pub current: String,
    pub next_patch: String,
    pub next_minor: String,
    pub next_major: String,
    pub total_commits: u32,
}

#[tauri::command]
pub async fn git_version_info(project_path: String) -> Result<VersionInfo, String> {
    // Get latest tag
    let current = run_in_dir("git describe --tags --abbrev=0 2>/dev/null", &project_path)
        .unwrap_or_else(|_| "v0.0.0".to_string());

    let version = current.trim_start_matches('v');
    let parts: Vec<u32> = version.split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect();

    let (major, minor, patch) = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );

    let total_commits = run_in_dir("git rev-list --count HEAD 2>/dev/null", &project_path)
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap_or(0);

    Ok(VersionInfo {
        current: current.clone(),
        next_patch: format!("v{}.{}.{}", major, minor, patch + 1),
        next_minor: format!("v{}.{}.0", major, minor + 1),
        next_major: format!("v{}.0.0", major + 1),
        total_commits,
    })
}

#[tauri::command]
pub async fn git_tag_version(project_path: String, version: String) -> Result<(), String> {
    let safe_version = version.replace('\'', "'\\''");
    run_in_dir(&format!("git tag '{}'", safe_version), &project_path)?;
    // Push the tag
    run_in_dir(&format!("git push origin '{}'", safe_version), &project_path).ok();
    Ok(())
}

/// One-click publish: stage all, commit, push, optionally tag
#[tauri::command]
pub async fn git_publish(
    project_path: String,
    message: String,
    auto_version: bool,
) -> Result<String, String> {
    // Stage all changes
    run_in_dir("git add -A", &project_path)?;

    // Check if there's anything to commit
    let status = run_in_dir("git status --porcelain", &project_path)?;
    if status.is_empty() {
        return Err("No changes to publish".to_string());
    }

    // Commit
    let safe_message = message.replace('\'', "'\\''");
    run_in_dir(&format!("git commit -m '{}'", safe_message), &project_path)?;

    // Auto-version: bump patch
    if auto_version {
        let version_info = git_version_info(project_path.clone()).await?;
        let new_tag = version_info.next_patch;
        run_in_dir(&format!("git tag '{}'", new_tag.replace('\'', "'\\''")), &project_path).ok();
    }

    // Push (with tags)
    let push_result = match run_in_dir("git push --follow-tags", &project_path) {
        Ok(r) => r,
        Err(_) => {
            let branch = run_in_dir("git branch --show-current", &project_path)
                .unwrap_or_else(|_| "main".to_string());
            run_in_dir(&format!("git push -u origin {} --follow-tags", branch), &project_path)?
        }
    };

    Ok(format!("Published successfully. {}", push_result))
}
