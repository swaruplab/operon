//! Linux platform implementations.
//! Very similar to macOS — differences: bash instead of zsh, xdg-open instead of open,
//! apt instead of brew, no Xcode, no osascript.

use crate::commands::claude::DependencyStatus;

// ─── Shell Execution ─────────────────────────────────────────────

pub fn shell_exec(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = std::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = tokio::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

// ─── Tool Discovery ──────────────────────────────────────────────

pub fn check_tool(name: &str) -> Option<(String, String)> {
    let which = shell_exec(&format!("which {}", name)).output().ok()?;
    if !which.status.success() { return None; }
    let path = String::from_utf8_lossy(&which.stdout).trim().to_string();
    let ver_out = shell_exec(&format!("{} --version", name)).output().ok()?;
    let version = String::from_utf8_lossy(&ver_out.stdout).trim().to_string();
    Some((path, version))
}

pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        super::operon_node_dir().join("bin"),
        std::path::PathBuf::from("/usr/local/bin"),
        home.join(".local/bin"),
        home.join(".claude/local/bin"),
        home.join(".npm-global/bin"),
    ]
}

// ─── Browser & OS Integration ────────────────────────────────────

pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;
    Ok(())
}

pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    // Try common terminal emulators in order of preference
    for term in &["gnome-terminal", "konsole", "xfce4-terminal", "xterm"] {
        let result = match *term {
            "gnome-terminal" => std::process::Command::new(term)
                .args(["--", "bash", "-c", &format!("{}; exec bash", command)])
                .spawn(),
            "konsole" => std::process::Command::new(term)
                .args(["-e", "bash", "-c", &format!("{}; exec bash", command)])
                .spawn(),
            _ => std::process::Command::new(term)
                .args(["-e", &format!("bash -c '{}; exec bash'", command)])
                .spawn(),
        };
        if result.is_ok() { return Ok(()); }
    }
    Err("No terminal emulator found. Please install gnome-terminal, konsole, or xterm.".to_string())
}

// ─── SSH ─────────────────────────────────────────────────────────
// ControlMaster works on Linux — identical to macOS.

pub fn ssh_mux_args(host: &str, port: u16, user: &str) -> String {
    let sock = super::ssh_socket_path(host, port, user);
    format!(
        " -o ControlMaster=auto -o ControlPath={} -o ControlPersist=4h",
        sock.display()
    )
}

pub fn ssh_mux_check(host: &str, port: u16, user: &str) -> bool {
    let sock = super::ssh_socket_path(host, port, user);
    let check_cmd = format!(
        "ssh -o ControlPath={} -O check {}@{} -p {} 2>/dev/null",
        sock.display(), user, host, port
    );
    shell_exec(&check_cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── Installation ────────────────────────────────────────────────

pub fn check_dependencies() -> DependencyStatus {
    DependencyStatus {
        xcode_cli: true, // Not applicable on Linux — always pass
        node: check_tool("node").is_some(),
        node_version: check_tool("node").map(|(_, v)| v),
        npm: check_tool("npm").is_some(),
        npm_version: check_tool("npm").map(|(_, v)| v),
        claude_code: check_tool("claude").is_some(),
        claude_version: check_tool("claude").map(|(_, v)| v),
        git_bash: true, // Not required on Linux — bash is always available
    }
}

pub fn install_node_platform() -> Result<(), String> {
    // Strategy 1: apt (if sudo is available)
    let has_sudo = std::process::Command::new("sudo")
        .args(["-n", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_sudo {
        let result = shell_exec("sudo apt-get install -y nodejs npm").output();
        if let Ok(o) = result {
            if o.status.success() { return Ok(()); }
        }
    }

    // Strategy 2: Tarball to operon data dir (no sudo needed)
    let arch = if cfg!(target_arch = "x86_64") { "x64" }
              else if cfg!(target_arch = "aarch64") { "arm64" }
              else { "x64" };
    let node_version = "v22.14.0";
    let tarball_url = format!(
        "https://nodejs.org/dist/{}/node-{}-linux-{}.tar.gz",
        node_version, node_version, arch
    );

    let dest = super::operon_node_dir();
    let tmp_tar = super::temp_dir().join("operon_node.tar.gz");

    let dl = std::process::Command::new("curl")
        .args(["-fSL", "--progress-bar", "-o"])
        .arg(&tmp_tar)
        .arg(&tarball_url)
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if !dl.status.success() {
        return Err(format!("Download failed: {}", String::from_utf8_lossy(&dl.stderr)));
    }

    if dest.exists() { let _ = std::fs::remove_dir_all(&dest); }
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    let extract = std::process::Command::new("tar")
        .args(["xzf"])
        .arg(&tmp_tar)
        .args(["--strip-components=1", "-C"])
        .arg(&dest)
        .output()
        .map_err(|e| format!("tar failed: {}", e))?;

    if !extract.status.success() {
        return Err(format!("Extract failed: {}", String::from_utf8_lossy(&extract.stderr)));
    }
    let _ = std::fs::remove_file(&tmp_tar);

    // Add to shell profile
    let home = dirs::home_dir().unwrap_or_default();
    let bin_dir = dest.join("bin");
    let path_line = format!("\nexport PATH=\"{}:$PATH\"\n", bin_dir.to_string_lossy());

    for profile_name in &[".bash_profile", ".profile"] {
        let profile_path = home.join(profile_name);
        if profile_path.exists() || *profile_name == ".profile" {
            if let Ok(existing) = std::fs::read_to_string(&profile_path) {
                if !existing.contains(".operon") {
                    let _ = std::fs::write(&profile_path, format!("{}{}", existing, path_line));
                }
            } else {
                let _ = std::fs::write(&profile_path, &path_line);
            }
            break;
        }
    }

    Ok(())
}

pub fn install_claude_platform() -> Result<(), String> {
    // Method 1: Official curl installer
    let output = shell_exec("curl -fsSL https://claude.ai/install.sh | bash").output();
    if let Ok(ref o) = output {
        if o.status.success() {
            return Ok(());
        }
    }

    // Method 2: npm
    let result = shell_exec("npm install -g @anthropic-ai/claude-code").output();
    match result {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("Install failed. Run: curl -fsSL https://claude.ai/install.sh | bash".to_string()),
    }
}

pub fn find_apt() -> Option<String> {
    let out = std::process::Command::new("which")
        .arg("apt-get")
        .output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

// ─── Menu ────────────────────────────────────────────────────────
// Linux menu is the same as Windows (no Services, Hide, etc.)

pub fn build_menu(
    app: &tauri::App,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, SubmenuBuilder, MenuItemBuilder, PredefinedMenuItem};

    let file_submenu = SubmenuBuilder::new(app, "File")
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    let edit_submenu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    let view_submenu = SubmenuBuilder::new(app, "View")
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    let help_item = MenuItemBuilder::with_id("open-help", "Operon Help")
        .build(app)?;

    let help_submenu = SubmenuBuilder::new(app, "Help")
        .item(&help_item)
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&help_submenu)
        .build()?;

    Ok(menu)
}
