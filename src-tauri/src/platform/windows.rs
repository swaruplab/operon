//! Windows platform implementations.

use crate::commands::claude::DependencyStatus;

// ─── Shell Execution ─────────────────────────────────────────────

pub fn shell_exec(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("cmd.exe");
    cmd.arg("/C").arg(command);
    cmd
}

pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("cmd.exe");
    cmd.arg("/C").arg(command);
    cmd
}

pub fn default_shell() -> String {
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

// ─── Tool Discovery ──────────────────────────────────────────────

pub fn check_tool(name: &str) -> Option<(String, String)> {
    let where_out = std::process::Command::new("where.exe")
        .arg(name)
        .output().ok()?;
    if !where_out.status.success() { return None; }
    let path = String::from_utf8_lossy(&where_out.stdout)
        .lines().next()?.trim().to_string();
    let ver_out = std::process::Command::new(&path)
        .arg("--version").output().ok()?;
    let version = String::from_utf8_lossy(&ver_out.stdout).trim().to_string();
    Some((path, version))
}

pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let appdata = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join("AppData").join("Roaming"));
    vec![
        super::operon_node_dir().join("bin"),
        appdata.join("npm"),
        std::path::PathBuf::from(r"C:\Program Files\nodejs"),
        std::path::PathBuf::from(r"C:\Program Files\Git\bin"),
        std::path::PathBuf::from(r"C:\Program Files\Git\cmd"),
        home.join(".claude\\local\\bin"),
    ]
}

/// Refresh the process's PATH environment variable from the Windows registry.
///
/// After winget/msi installs, the system PATH is updated but our running
/// process still has the old PATH. This reads the current User + Machine
/// PATH values from the registry and updates the process environment.
pub fn refresh_path_from_registry() {
    let machine_path = read_registry_path(
        "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        "Path",
    );
    let user_path = read_registry_path("HKCU\\Environment", "Path");

    let extra: Vec<String> = super::extra_tool_paths()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let new_path = format!(
        "{};{};{}",
        extra.join(";"),
        machine_path.unwrap_or_default(),
        user_path.unwrap_or_default()
    );
    std::env::set_var("PATH", &new_path);
}

fn read_registry_path(key: &str, value: &str) -> Option<String> {
    let output = std::process::Command::new("reg")
        .args(["query", key, "/v", value])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // reg query output: "    Path    REG_EXPAND_SZ    C:\...;C:\..."
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(value) || trimmed.contains("REG_EXPAND_SZ") || trimmed.contains("REG_SZ") {
            // Split on REG_EXPAND_SZ or REG_SZ and take the value part
            if let Some(pos) = trimmed.find("REG_EXPAND_SZ") {
                return Some(trimmed[pos + "REG_EXPAND_SZ".len()..].trim().to_string());
            }
            if let Some(pos) = trimmed.find("REG_SZ") {
                return Some(trimmed[pos + "REG_SZ".len()..].trim().to_string());
            }
        }
    }
    None
}

// ─── Browser & OS Integration ────────────────────────────────────

pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("cmd.exe")
        .args(["/C", "start", "", url])
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;
    Ok(())
}

pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    // Set CLAUDE_CODE_GIT_BASH_PATH so `claude login` can find bash
    let git_bash_env = find_git_bash();

    // Strategy 1: Windows Terminal (wt.exe) — modern Windows 10/11
    let mut wt = std::process::Command::new("wt.exe");
    wt.args(["new-tab", "cmd.exe", "/K", command]);
    if let Some(ref bash_path) = git_bash_env {
        wt.env("CLAUDE_CODE_GIT_BASH_PATH", bash_path);
    }
    if wt.spawn().is_ok() {
        return Ok(());
    }

    // Strategy 2: PowerShell via full path in System32
    let system32 = std::env::var("SYSTEMROOT")
        .unwrap_or_else(|_| r"C:\Windows".to_string());
    let ps_path = format!(r"{}\System32\WindowsPowerShell\v1.0\powershell.exe", system32);

    let mut ps = std::process::Command::new(&ps_path);
    ps.args(["-NoExit", "-Command", command]);
    if let Some(ref bash_path) = git_bash_env {
        ps.env("CLAUDE_CODE_GIT_BASH_PATH", bash_path);
    }
    if ps.spawn().is_ok() {
        return Ok(());
    }

    // Strategy 3: PowerShell via PATH (fallback)
    let mut ps2 = std::process::Command::new("powershell.exe");
    ps2.args(["-NoExit", "-Command", command]);
    if let Some(ref bash_path) = git_bash_env {
        ps2.env("CLAUDE_CODE_GIT_BASH_PATH", bash_path);
    }
    if ps2.spawn().is_ok() {
        return Ok(());
    }

    // Strategy 4: cmd.exe (always available)
    let mut cmd = std::process::Command::new("cmd.exe");
    cmd.args(["/K", command]);
    if let Some(ref bash_path) = git_bash_env {
        cmd.env("CLAUDE_CODE_GIT_BASH_PATH", bash_path);
    }
    cmd.spawn()
        .map_err(|e| format!("Failed to open any terminal (tried wt, PowerShell, cmd): {}", e))?;
    Ok(())
}

// ─── SSH ─────────────────────────────────────────────────────────
// ControlMaster not supported on Windows. SSH multiplexing uses ssh-agent service.
// ssh_mux_args and ssh_mux_check are handled in mod.rs (return empty/false).

// ─── Git Bash (required by Claude Code on Windows) ──────────────

/// Find the Git Bash executable path.
/// Checks common install locations, user-level installs, and the PATH.
pub fn find_git_bash() -> Option<String> {
    let home = dirs::home_dir().unwrap_or_default();
    let localappdata = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join("AppData").join("Local"));

    let candidates = [
        // Standard system-wide install
        r"C:\Program Files\Git\bin\bash.exe".to_string(),
        r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
        // User-level / winget install locations
        format!(r"{}\Programs\Git\bin\bash.exe", localappdata.display()),
        format!(r"{}\Git\bin\bash.exe", localappdata.display()),
        // Scoop
        format!(r"{}\scoop\apps\git\current\bin\bash.exe", home.display()),
        // Chocolatey
        r"C:\ProgramData\chocolatey\lib\git\tools\bin\bash.exe".to_string(),
        // PortableGit
        format!(r"{}\PortableGit\bin\bash.exe", home.display()),
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Check if CLAUDE_CODE_GIT_BASH_PATH is already set (e.g. by user or previous persist)
    if let Ok(path) = std::env::var("CLAUDE_CODE_GIT_BASH_PATH") {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    // Check PATH via where.exe
    let where_out = std::process::Command::new("where.exe")
        .arg("bash.exe")
        .output().ok()?;
    if where_out.status.success() {
        // where.exe may return multiple results; prefer one inside a Git directory
        for line in String::from_utf8_lossy(&where_out.stdout).lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && (trimmed.to_lowercase().contains("git")) {
                return Some(trimmed.to_string());
            }
        }
        // If no Git-specific one, take the first result
        let first = String::from_utf8_lossy(&where_out.stdout)
            .lines().next()?.trim().to_string();
        if !first.is_empty() {
            return Some(first);
        }
    }

    None
}

/// Check if Git (and Git Bash) is installed.
pub fn has_git_bash() -> bool {
    find_git_bash().is_some()
}

/// Install Git for Windows.
///
/// Downloads the official Git installer using `certutil` (built into every
/// Windows since Vista — no PowerShell needed) and launches it with the GUI
/// so the user can click through the wizard. The installer handles its own
/// UAC elevation prompt.
///
/// Returns Ok(()) if the installer was launched (NOT that Git is installed —
/// caller must re-check after the user finishes the wizard).
/// Returns Err("INSTALLER_LAUNCHED") as a sentinel so the caller knows to
/// show "re-check" UI.
pub fn install_git() -> Result<(), String> {
    let temp = super::temp_dir();
    let installer_path = temp.join("Git-installer.exe");
    let installer_str = installer_path.to_string_lossy().to_string();

    // 64-bit standalone installer URL (works on all modern Windows)
    let url = "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.2/Git-2.47.1.2-64-bit.exe";

    eprintln!("[Git] Downloading installer from {}", url);

    // Strategy 1: certutil (built into Windows, most reliable, no PowerShell dep)
    let dl_result = std::process::Command::new("certutil")
        .args(["-urlcache", "-split", "-f", url, &installer_str])
        .output();

    let downloaded = match dl_result {
        Ok(o) if o.status.success() && installer_path.exists() => {
            eprintln!("[Git] certutil download succeeded");
            true
        }
        Ok(o) => {
            eprintln!("[Git] certutil failed: {}", String::from_utf8_lossy(&o.stderr));
            false
        }
        Err(e) => {
            eprintln!("[Git] certutil not available: {}", e);
            false
        }
    };

    // Strategy 2: PowerShell Invoke-WebRequest fallback
    let downloaded = if downloaded { true } else {
        eprintln!("[Git] Trying PowerShell download...");
        let ps_cmd = format!(
            "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
            url, installer_str
        );
        let ps_result = std::process::Command::new("powershell.exe")
            .args(["-ExecutionPolicy", "Bypass", "-Command", &ps_cmd])
            .output();

        match ps_result {
            Ok(o) if o.status.success() && installer_path.exists() => {
                eprintln!("[Git] PowerShell download succeeded");
                true
            }
            _ => false,
        }
    };

    // Strategy 3: bitsadmin fallback (also built into Windows)
    let downloaded = if downloaded { true } else {
        eprintln!("[Git] Trying bitsadmin download...");
        let bits_result = std::process::Command::new("bitsadmin")
            .args(["/transfer", "GitDownload", "/download", "/priority", "high", url, &installer_str])
            .output();

        match bits_result {
            Ok(o) if o.status.success() && installer_path.exists() => {
                eprintln!("[Git] bitsadmin download succeeded");
                true
            }
            _ => false,
        }
    };

    if downloaded && installer_path.exists() {
        eprintln!("[Git] Launching installer GUI: {}", installer_str);
        // Launch the installer with GUI — it will prompt for UAC itself
        match std::process::Command::new(&installer_str)
            .spawn()
        {
            Ok(_) => {
                eprintln!("[Git] Installer launched successfully");
                return Err("INSTALLER_LAUNCHED".to_string());
            }
            Err(e) => {
                eprintln!("[Git] Failed to launch installer: {}", e);
                // Try via cmd /C start (handles UAC better sometimes)
                let _ = std::process::Command::new("cmd.exe")
                    .args(["/C", "start", "", &installer_str])
                    .spawn();
                return Err("INSTALLER_LAUNCHED".to_string());
            }
        }
    }

    // All download strategies failed — fall back to opening the browser
    eprintln!("[Git] All download strategies failed, opening browser");
    let _ = open_url("https://git-scm.com/downloads/win");
    Err("BROWSER_OPENED".to_string())
}

/// Persist CLAUDE_CODE_GIT_BASH_PATH as a user-level environment variable
/// so that Claude Code can always find Git Bash, even from external terminals.
pub fn persist_git_bash_env() {
    if let Some(bash_path) = find_git_bash() {
        // Set it for the current process
        std::env::set_var("CLAUDE_CODE_GIT_BASH_PATH", &bash_path);

        // Persist it as a user-level env var via setx (survives reboots)
        let _ = std::process::Command::new("setx")
            .args(["CLAUDE_CODE_GIT_BASH_PATH", &bash_path])
            .output();

        eprintln!("[Git] Set CLAUDE_CODE_GIT_BASH_PATH={}", bash_path);
    }
}

// ─── Installation ────────────────────────────────────────────────

pub fn check_dependencies() -> DependencyStatus {
    DependencyStatus {
        xcode_cli: true, // Not applicable on Windows — always pass
        node: check_tool("node").is_some(),
        node_version: check_tool("node").map(|(_, v)| v),
        npm: check_tool("npm").is_some(),
        npm_version: check_tool("npm").map(|(_, v)| v),
        claude_code: check_tool("claude").is_some(),
        claude_version: check_tool("claude").map(|(_, v)| v),
        git_bash: has_git_bash(),
    }
}

pub fn install_node_platform() -> Result<(), String> {
    // Strategy 1: winget (built into Windows 11 and Windows 10 1709+)
    let winget = std::process::Command::new("winget")
        .args(["install", "--id", "OpenJS.NodeJS.LTS",
               "--accept-source-agreements", "--accept-package-agreements"])
        .output();

    if let Ok(o) = winget {
        if o.status.success() {
            refresh_path_from_registry();
            return Ok(());
        }
        let out_text = format!("{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr));
        if out_text.contains("already installed") {
            refresh_path_from_registry();
            return Ok(());
        }
    }

    // Strategy 2: Download .msi installer via PowerShell and run silently
    let arch = if cfg!(target_arch = "x86_64") { "x64" } else { "arm64" };
    let url = format!(
        "https://nodejs.org/dist/v22.14.0/node-v22.14.0-{}.msi", arch
    );
    let msi_path = super::temp_dir().join("node-installer.msi");
    let msi_str = msi_path.to_string_lossy().to_string();

    // Download with PowerShell
    let download_cmd = format!(
        "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
        url, msi_str
    );
    let dl = std::process::Command::new("powershell.exe")
        .args(["-ExecutionPolicy", "Bypass", "-Command", &download_cmd])
        .output();

    if let Ok(o) = dl {
        if o.status.success() && msi_path.exists() {
            // Run MSI installer silently
            let install = std::process::Command::new("msiexec")
                .args(["/i", &msi_str, "/qn", "/norestart"])
                .output();
            // Clean up the MSI
            let _ = std::fs::remove_file(&msi_path);
            if let Ok(o) = install {
                if o.status.success() {
                    refresh_path_from_registry();
                    return Ok(());
                }
            }
        }
    }

    Err("Automatic Node.js install failed. Please install from https://nodejs.org/ and restart Operon.".to_string())
}

pub fn install_claude_platform() -> Result<(), String> {
    // Refresh PATH first so we can find npm/node installed moments ago
    refresh_path_from_registry();

    // Strategy 1: Official curl installer via Git Bash (most reliable)
    if let Some(bash_path) = find_git_bash() {
        let result = std::process::Command::new(&bash_path)
            .args(["-c", "curl -fsSL https://claude.ai/install.sh | bash"])
            .output();
        if let Ok(o) = result {
            if o.status.success() {
                refresh_path_from_registry();
                return Ok(());
            }
        }
    }

    // Strategy 2: npm install (requires Node.js in PATH)
    let result = shell_exec("npm install -g @anthropic-ai/claude-code").output();
    match result {
        Ok(o) if o.status.success() => {
            refresh_path_from_registry();
            Ok(())
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(format!("npm install failed: {}. Run: npm install -g @anthropic-ai/claude-code", stderr.chars().take(200).collect::<String>()))
        }
        Err(e) => Err(format!("Could not run npm: {}. Ensure Node.js is installed and restart Operon.", e)),
    }
}

pub fn find_winget() -> Option<String> {
    let out = std::process::Command::new("where.exe")
        .arg("winget")
        .output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string())
    } else {
        None
    }
}

// ─── Python ─────────────────────────────────────────────────────

/// Find the Python executable on Windows.
/// Windows uses "python" (not "python3") — the Microsoft Store alias or installer.
pub fn find_python() -> Option<String> {
    // Check "python" first (standard Windows name)
    if let Some((path, _)) = check_tool("python") {
        return Some(path);
    }
    // Fallback: "python3" (some installers add this)
    if let Some((path, _)) = check_tool("python3") {
        return Some(path);
    }
    None
}

/// Install Python via winget.
pub fn install_python() -> Result<(), String> {
    let winget = std::process::Command::new("winget")
        .args(["install", "--id", "Python.Python.3.12", "-e",
               "--accept-source-agreements", "--accept-package-agreements"])
        .output();

    if let Ok(o) = winget {
        if o.status.success() {
            return Ok(());
        }
        let out_text = format!("{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr));
        if out_text.contains("already installed") {
            return Ok(());
        }
    }

    Err("Python could not be installed automatically. Please install from https://python.org/downloads and restart Operon.".to_string())
}

// ─── OpenSSH ────────────────────────────────────────────────────

/// Check if OpenSSH client is available.
pub fn has_openssh() -> bool {
    // Check if ssh.exe is on PATH
    check_tool("ssh").is_some()
}

/// Enable the OpenSSH client Windows optional feature.
/// This requires admin privileges on older Windows 10 builds.
/// Windows 11 typically has it enabled by default.
pub fn install_openssh() -> Result<(), String> {
    // Strategy 1: winget (works on Windows 11)
    let winget = std::process::Command::new("winget")
        .args(["install", "--id", "Microsoft.OpenSSH.Beta", "-e",
               "--accept-source-agreements", "--accept-package-agreements"])
        .output();

    if let Ok(o) = winget {
        let out_text = format!("{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr));
        if o.status.success() || out_text.contains("already installed") {
            return Ok(());
        }
    }

    // Strategy 2: PowerShell Add-WindowsCapability (requires admin)
    let ps = std::process::Command::new("powershell.exe")
        .args(["-Command", "Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0"])
        .output();

    if let Ok(o) = ps {
        if o.status.success() {
            return Ok(());
        }
    }

    Err("OpenSSH could not be installed automatically. Enable it in Settings → Apps → Optional Features → OpenSSH Client.".to_string())
}

// ─── uv (Python package manager, provides uvx) ─────────────────

/// Check if uv/uvx is installed.
pub fn has_uv() -> bool {
    check_tool("uvx").is_some() || check_tool("uv").is_some()
}

/// Install uv via the official standalone installer (does not require Python).
pub fn install_uv() -> Result<(), String> {
    // Strategy 1: PowerShell standalone installer (recommended, no Python needed)
    let ps = std::process::Command::new("powershell.exe")
        .args(["-ExecutionPolicy", "ByPass", "-Command",
               "irm https://astral.sh/uv/install.ps1 | iex"])
        .output();

    if let Ok(o) = ps {
        if o.status.success() {
            return Ok(());
        }
    }

    // Strategy 2: winget
    let winget = std::process::Command::new("winget")
        .args(["install", "--id", "astral-sh.uv", "-e",
               "--accept-source-agreements", "--accept-package-agreements"])
        .output();

    if let Ok(o) = winget {
        let out_text = format!("{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr));
        if o.status.success() || out_text.contains("already installed") {
            return Ok(());
        }
    }

    // Strategy 3: pip install uv (requires Python)
    if find_python().is_some() {
        let pip = shell_exec("pip install uv").output();
        if let Ok(o) = pip {
            if o.status.success() { return Ok(()); }
        }
    }

    Err("uv could not be installed automatically. Install from https://docs.astral.sh/uv/getting-started/installation/".to_string())
}

// ─── reportlab (Python PDF library) ─────────────────────────────

/// Check if reportlab is installed.
pub fn has_reportlab() -> bool {
    let python = find_python().unwrap_or_else(|| "python".to_string());
    std::process::Command::new(&python)
        .args(["-c", "import reportlab"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Install reportlab via pip.
pub fn install_reportlab() -> Result<(), String> {
    let python = find_python()
        .ok_or("Python is not installed — cannot install reportlab.")?;

    // Strategy 1: pip install --user
    if let Ok(o) = std::process::Command::new(&python)
        .args(["-m", "pip", "install", "reportlab", "--user", "--quiet"])
        .output()
    {
        if o.status.success() { return Ok(()); }
    }

    // Strategy 2: pip install (no --user)
    if let Ok(o) = std::process::Command::new(&python)
        .args(["-m", "pip", "install", "reportlab", "--quiet"])
        .output()
    {
        if o.status.success() { return Ok(()); }
    }

    Err("reportlab could not be installed. Run: pip install reportlab".to_string())
}

// ─── File System ─────────────────────────────────────────────────

/// Check if a file is hidden on Windows using the Hidden file attribute.
pub fn is_hidden(path: &std::path::Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata(path) {
            // FILE_ATTRIBUTE_HIDDEN = 0x2
            meta.file_attributes() & 0x2 != 0
        } else {
            false
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

// ─── Menu ────────────────────────────────────────────────────────

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
