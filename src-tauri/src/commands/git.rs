use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

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

    let branch = run_in_dir("git branch --show-current", &project_path).unwrap_or_default();

    // Count changed, staged, untracked
    let status_output = run_in_dir("git status --porcelain", &project_path).unwrap_or_default();

    let mut changed: u32 = 0;
    let mut staged: u32 = 0;
    let mut untracked: u32 = 0;

    for line in status_output.lines() {
        if line.len() < 2 {
            continue;
        }
        let xy: Vec<char> = line.chars().take(2).collect();
        if xy[0] == '?' {
            untracked += 1;
        } else {
            if xy[0] != ' ' && xy[0] != '?' {
                staged += 1;
            }
            if xy[1] != ' ' && xy[1] != '?' {
                changed += 1;
            }
        }
    }

    // Ahead/behind
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    if let Ok(ab) = run_in_dir(
        "git rev-list --left-right --count HEAD...@{upstream} 2>/dev/null",
        &project_path,
    ) {
        let parts: Vec<&str> = ab.split_whitespace().collect();
        if parts.len() == 2 {
            ahead = parts[0].parse().unwrap_or(0);
            behind = parts[1].parse().unwrap_or(0);
        }
    }

    // Remote URL
    let remote_url =
        run_in_dir("git remote get-url origin 2>/dev/null", &project_path).unwrap_or_default();
    let has_remote = !remote_url.is_empty();

    // Last commit
    let last_commit_message =
        run_in_dir("git log -1 --format=%s 2>/dev/null", &project_path).unwrap_or_default();
    let last_commit_time =
        run_in_dir("git log -1 --format=%ar 2>/dev/null", &project_path).unwrap_or_default();

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
        + String::from_utf8_lossy(&auth_output.stderr).as_ref();

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
            format!(
                "{} install --id GitHub.cli --accept-source-agreements --accept-package-agreements",
                pkg_mgr
            )
        } else {
            // apt
            "sudo apt-get install -y gh".to_string()
        };
        let tmp = crate::platform::temp_dir().to_string_lossy().to_string();
        run_in_dir(&cmd, &tmp)?;
        Ok(())
    } else {
        Err(
            "No package manager found. Please install GitHub CLI manually: https://cli.github.com"
                .to_string(),
        )
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
                + String::from_utf8_lossy(&o.stderr).as_ref()
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
            for line in reader.lines().map_while(Result::ok) {
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

        // Wait for process to complete
        let _ = child.wait();

        // Notify frontend that login completed
        if let Some(window) = app.get_webview_window("main") {
            // Check final auth status
            let ok = crate::platform::shell_exec("gh auth status 2>&1")
                .output()
                .map(|o| {
                    let out = String::from_utf8_lossy(&o.stdout).to_string()
                        + String::from_utf8_lossy(&o.stderr).as_ref();
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
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap_or(0)).collect();

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
    run_in_dir(
        &format!("git push origin '{}'", safe_version),
        &project_path,
    )
    .ok();
    Ok(())
}

/// One-click publish: stage all, commit, push, optionally tag
#[tauri::command]
pub async fn git_publish(
    project_path: String,
    message: String,
    auto_version: bool,
    version_tag: Option<String>,
    target_branch: Option<String>,
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

    // Tagging: explicit tag overrides auto-version
    if let Some(tag) = version_tag {
        if !tag.is_empty() {
            run_in_dir(
                &format!("git tag '{}'", tag.replace('\'', "'\\''")),
                &project_path,
            )
            .ok();
        }
    } else if auto_version {
        let version_info = git_version_info(project_path.clone()).await?;
        let new_tag = version_info.next_patch;
        run_in_dir(
            &format!("git tag '{}'", new_tag.replace('\'', "'\\''")),
            &project_path,
        )
        .ok();
    }

    // Determine which branch to push
    let local_branch = run_in_dir("git branch --show-current", &project_path)
        .unwrap_or_else(|_| "main".to_string());
    let push_target = target_branch.unwrap_or_else(|| local_branch.clone());
    let safe_target = push_target.replace('\'', "'\\''");

    // Push (with tags). If local and remote branch names differ, use refspec.
    let push_cmd = if local_branch == push_target {
        format!("git push -u origin '{}' --follow-tags", safe_target)
    } else {
        format!(
            "git push -u origin '{}:{}' --follow-tags",
            local_branch, safe_target
        )
    };

    let push_result = run_in_dir(&push_cmd, &project_path)?;

    Ok(format!(
        "Published to {} successfully. {}",
        push_target, push_result
    ))
}

// ──────────────────────────────────────────────
// Repository listing & remote management
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GhRepo {
    pub name: String,
    pub full_name: String, // owner/repo
    pub private: bool,
    pub url: String,
    pub description: String,
}

/// List the authenticated user's GitHub repos (up to 100, sorted by most recent push).
#[tauri::command]
pub async fn gh_list_repos() -> Result<Vec<GhRepo>, String> {
    let output = crate::platform::shell_exec(
        "gh repo list --limit 100 --json name,nameWithOwner,isPrivate,url,description",
    )
    .output()
    .map_err(|e| format!("Failed to list repos: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Failed to list repos: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse repo list: {}", e))?;

    let repos = raw
        .iter()
        .map(|r| GhRepo {
            name: r["name"].as_str().unwrap_or("").to_string(),
            full_name: r["nameWithOwner"].as_str().unwrap_or("").to_string(),
            private: r["isPrivate"].as_bool().unwrap_or(false),
            url: r["url"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    Ok(repos)
}

/// Add a remote pointing to an existing GitHub repository.
#[tauri::command]
pub async fn gh_add_remote(
    project_path: String,
    remote_url: String,
    remote_name: Option<String>,
) -> Result<(), String> {
    let name = remote_name.unwrap_or_else(|| "origin".to_string());
    let safe_url = remote_url.replace('\'', "'\\''");
    let safe_name = name.replace('\'', "'\\''");

    // Remove existing remote if it exists, then add the new one
    run_in_dir(
        &format!(
            "git remote remove '{}' 2>/dev/null; git remote add '{}' '{}'",
            safe_name, safe_name, safe_url
        ),
        &project_path,
    )?;
    Ok(())
}

/// List local and remote branches and return the current one.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BranchInfo {
    pub current: String,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
}

#[tauri::command]
pub async fn git_list_branches(project_path: String) -> Result<BranchInfo, String> {
    // Fetch latest remote refs (ignore errors — may be offline)
    run_in_dir("git fetch --prune 2>/dev/null", &project_path).ok();

    let raw = run_in_dir("git branch --no-color", &project_path)?;
    let mut current = String::new();
    let mut branches = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with("* ") {
            let name = line.trim_start_matches("* ").to_string();
            current = name.clone();
            branches.push(name);
        } else if !line.is_empty() {
            branches.push(line.to_string());
        }
    }

    // Remote branches: strip "origin/" prefix, deduplicate with local
    let mut remote_branches = Vec::new();
    if let Ok(remote_raw) = run_in_dir("git branch -r --no-color", &project_path) {
        for line in remote_raw.lines() {
            let line = line.trim();
            if line.contains("->") {
                continue;
            } // skip HEAD -> origin/main
              // Strip remote prefix: "origin/main" → "main"
            let name = line.split('/').skip(1).collect::<Vec<_>>().join("/");
            if !name.is_empty() && !branches.contains(&name) {
                remote_branches.push(name);
            }
        }
    }
    remote_branches.sort();
    remote_branches.dedup();

    Ok(BranchInfo {
        current,
        branches,
        remote_branches,
    })
}

/// Switch to a branch (create it if it doesn't exist).
#[tauri::command]
pub async fn git_switch_branch(
    project_path: String,
    branch: String,
    create: bool,
) -> Result<(), String> {
    let safe_branch = branch.replace('\'', "'\\''");
    if create {
        run_in_dir(&format!("git checkout -b '{}'", safe_branch), &project_path)?;
    } else {
        run_in_dir(&format!("git checkout '{}'", safe_branch), &project_path)?;
    }
    Ok(())
}

/// Pull from remote (fetch + merge).
#[tauri::command]
pub async fn git_pull(project_path: String) -> Result<String, String> {
    run_in_dir("git pull --ff-only", &project_path)
}

// ──────────────────────────────────────────────
// Changed file list with staging
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: String, // "M" modified, "A" added, "D" deleted, "R" renamed, "?" untracked
    pub staged: bool,
}

/// List individual changed files with their staging status.
#[tauri::command]
pub async fn git_changed_files(project_path: String) -> Result<Vec<ChangedFile>, String> {
    let raw = run_in_dir("git status --porcelain", &project_path)?;
    let mut files = Vec::new();
    for line in raw.lines() {
        if line.len() < 3 {
            continue;
        }
        let index_status = line.chars().next().unwrap_or(' ');
        let worktree_status = line.chars().nth(1).unwrap_or(' ');
        let path = line[3..].trim().to_string();
        // Remove quotes from paths with special chars
        let path = path.trim_matches('"').to_string();

        if index_status == '?' {
            // Untracked file
            files.push(ChangedFile {
                path,
                status: "?".to_string(),
                staged: false,
            });
        } else {
            // If index has a status, it's staged
            if index_status != ' ' {
                files.push(ChangedFile {
                    path: path.clone(),
                    status: index_status.to_string(),
                    staged: true,
                });
            }
            // If worktree has a status, there are unstaged changes too
            if worktree_status != ' ' && worktree_status != '?' {
                files.push(ChangedFile {
                    path,
                    status: worktree_status.to_string(),
                    staged: false,
                });
            }
        }
    }
    Ok(files)
}

/// Stage specific files.
#[tauri::command]
pub async fn git_stage_files(project_path: String, paths: Vec<String>) -> Result<(), String> {
    for path in &paths {
        let safe_path = path.replace('\'', "'\\''");
        run_in_dir(&format!("git add '{}'", safe_path), &project_path)?;
    }
    Ok(())
}

/// Unstage specific files.
#[tauri::command]
pub async fn git_unstage_files(project_path: String, paths: Vec<String>) -> Result<(), String> {
    for path in &paths {
        let safe_path = path.replace('\'', "'\\''");
        run_in_dir(&format!("git reset HEAD -- '{}'", safe_path), &project_path)?;
    }
    Ok(())
}

/// Discard changes in specific files (revert to HEAD).
#[tauri::command]
pub async fn git_discard_files(project_path: String, paths: Vec<String>) -> Result<(), String> {
    for path in &paths {
        let safe_path = path.replace('\'', "'\\''");
        // For untracked files, remove them; for tracked, checkout from HEAD
        let is_untracked = run_in_dir(
            &format!("git ls-files --error-unmatch '{}' 2>/dev/null", safe_path),
            &project_path,
        )
        .is_err();
        if is_untracked {
            run_in_dir(&format!("rm -f '{}'", safe_path), &project_path)?;
        } else {
            run_in_dir(
                &format!("git checkout HEAD -- '{}'", safe_path),
                &project_path,
            )?;
        }
    }
    Ok(())
}

// ──────────────────────────────────────────────
// Stash
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StashEntry {
    pub index: u32,
    pub message: String,
    pub date: String,
}

/// List stash entries.
#[tauri::command]
pub async fn git_stash_list(project_path: String) -> Result<Vec<StashEntry>, String> {
    let raw =
        run_in_dir("git stash list --format='%gd|||%gs|||%ar'", &project_path).unwrap_or_default();
    let mut entries = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split("|||").collect();
        if parts.len() >= 3 {
            let index: u32 = parts[0]
                .replace("stash@{", "")
                .replace("}", "")
                .parse()
                .unwrap_or(0);
            entries.push(StashEntry {
                index,
                message: parts[1].to_string(),
                date: parts[2].to_string(),
            });
        }
    }
    Ok(entries)
}

/// Stash current changes.
#[tauri::command]
pub async fn git_stash_save(project_path: String, message: Option<String>) -> Result<(), String> {
    let cmd = match message {
        Some(msg) if !msg.is_empty() => {
            format!("git stash push -m '{}'", msg.replace('\'', "'\\''"))
        }
        _ => "git stash push".to_string(),
    };
    run_in_dir(&cmd, &project_path)?;
    Ok(())
}

/// Pop a stash entry (apply + drop).
#[tauri::command]
pub async fn git_stash_pop(project_path: String, index: Option<u32>) -> Result<(), String> {
    let cmd = match index {
        Some(i) => format!("git stash pop stash@{{{}}}", i),
        None => "git stash pop".to_string(),
    };
    run_in_dir(&cmd, &project_path)?;
    Ok(())
}

/// Drop a stash entry.
#[tauri::command]
pub async fn git_stash_drop(project_path: String, index: u32) -> Result<(), String> {
    run_in_dir(
        &format!("git stash drop stash@{{{}}}", index),
        &project_path,
    )?;
    Ok(())
}

// ──────────────────────────────────────────────
// Commit history
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitEntry {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub files_changed: u32,
}

/// Get recent commit history.
#[tauri::command]
pub async fn git_log(project_path: String, count: Option<u32>) -> Result<Vec<CommitEntry>, String> {
    let n = count.unwrap_or(30);
    let raw = run_in_dir(
        &format!(
            "git log --format='%H|||%h|||%s|||%an|||%ar|||' --shortstat -n {}",
            n
        ),
        &project_path,
    )?;

    let mut entries = Vec::new();
    let mut current: Option<(String, String, String, String, String)> = None;

    for line in raw.lines() {
        let line = line.trim();
        if line.contains("|||") {
            // Save previous entry
            if let Some((hash, short, msg, author, date)) = current.take() {
                entries.push(CommitEntry {
                    hash,
                    short_hash: short,
                    message: msg,
                    author,
                    date,
                    files_changed: 0,
                });
            }
            let parts: Vec<&str> = line.split("|||").collect();
            if parts.len() >= 5 {
                current = Some((
                    parts[0].to_string(),
                    parts[1].to_string(),
                    parts[2].to_string(),
                    parts[3].to_string(),
                    parts[4].to_string(),
                ));
            }
        } else if line.contains("file") && line.contains("changed") {
            // shortstat line: " 3 files changed, 10 insertions(+), 2 deletions(-)"
            let files: u32 = line
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if let Some((hash, short, msg, author, date)) = current.take() {
                entries.push(CommitEntry {
                    hash,
                    short_hash: short,
                    message: msg,
                    author,
                    date,
                    files_changed: files,
                });
            }
        }
    }
    // Push the last entry if no shortstat followed
    if let Some((hash, short, msg, author, date)) = current {
        entries.push(CommitEntry {
            hash,
            short_hash: short,
            message: msg,
            author,
            date,
            files_changed: 0,
        });
    }

    Ok(entries)
}

/// Get diff for a specific commit.
#[tauri::command]
pub async fn git_show_commit(project_path: String, hash: String) -> Result<String, String> {
    let safe_hash = hash.replace('\'', "'\\''");
    run_in_dir(
        &format!("git show --stat '{}' 2>/dev/null", safe_hash),
        &project_path,
    )
}

/// Amend the last commit with currently staged changes and optional new message.
#[tauri::command]
pub async fn git_amend(project_path: String, message: Option<String>) -> Result<(), String> {
    let cmd = match message {
        Some(msg) if !msg.is_empty() => {
            format!("git commit --amend -m '{}'", msg.replace('\'', "'\\''"))
        }
        _ => "git commit --amend --no-edit".to_string(),
    };
    run_in_dir(&cmd, &project_path)?;
    Ok(())
}
