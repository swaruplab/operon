//! Platform Abstraction Layer
//!
//! ALL OS-specific code lives here. Command files should never contain
//! `#[cfg(target_os = ...)]`, `osascript`, `/bin/zsh`, `cmd.exe`, or
//! any OS-specific path. They call functions from this module instead.

pub mod common;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub use common::*;

// ─── Shell Execution ─────────────────────────────────────────────

/// Run a command string through the OS's login shell.
///
/// macOS:   /bin/zsh -l -c "command"
/// Windows: cmd.exe /C "command"
/// Linux:   /bin/bash -l -c "command"
///
/// This is the ONLY way command files should execute shell commands.
/// Never construct a Command::new("/bin/zsh") directly.
pub fn shell_exec(command: &str) -> std::process::Command {
    #[cfg(target_os = "macos")]
    {
        macos::shell_exec(command)
    }
    #[cfg(target_os = "windows")]
    {
        windows::shell_exec(command)
    }
    #[cfg(target_os = "linux")]
    {
        linux::shell_exec(command)
    }
}

/// Async version for use in async Tauri commands.
pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    #[cfg(target_os = "macos")]
    {
        macos::shell_exec_async(command)
    }
    #[cfg(target_os = "windows")]
    {
        windows::shell_exec_async(command)
    }
    #[cfg(target_os = "linux")]
    {
        linux::shell_exec_async(command)
    }
}

/// The default interactive shell for terminal spawning.
///
/// macOS:   $SHELL or /bin/zsh
/// Windows: $COMSPEC or cmd.exe
/// Linux:   $SHELL or /bin/bash
pub fn default_shell() -> String {
    #[cfg(target_os = "macos")]
    {
        macos::default_shell()
    }
    #[cfg(target_os = "windows")]
    {
        windows::default_shell()
    }
    #[cfg(target_os = "linux")]
    {
        linux::default_shell()
    }
}

// ─── Directories ─────────────────────────────────────────────────

/// The user's home directory.
pub fn home_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir()
}

/// Operon's application data directory (sessions, SSH profiles, caches).
///
/// macOS:   ~/Library/Application Support/operon/
/// Windows: C:\Users\{user}\AppData\Local\operon\
/// Linux:   ~/.local/share/operon/
pub fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join("operon")
}

/// Operon's configuration directory (settings.json).
///
/// macOS:   ~/Library/Application Support/operon/   (via dirs::config_dir)
/// Windows: C:\Users\{user}\AppData\Roaming\operon\
/// Linux:   ~/.config/operon/
pub fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join("operon")
}

/// Session metadata directory.
pub fn sessions_dir() -> Result<std::path::PathBuf, String> {
    let dir = data_dir().join("sessions");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create sessions dir: {}", e))?;
    }
    Ok(dir)
}

/// SSH socket/multiplexing directory.
/// IMPORTANT: This MUST be a path with no spaces — OpenSSH's ControlPath
/// breaks on paths containing spaces (like ~/Library/Application Support/).
/// We use ~/.operon/sockets/ instead of the standard data_dir().
pub fn ssh_sockets_dir() -> std::path::PathBuf {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".operon")
        .join("sockets");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Temp directory (for install scripts, etc.)
pub fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

/// The Operon-managed Node.js installation directory.
pub fn operon_node_dir() -> std::path::PathBuf {
    data_dir().join("node")
}

// ─── Tool Discovery ──────────────────────────────────────────────

/// Check if a CLI tool is installed and return (path, version).
///
/// macOS/Linux: `which {name}` then `{name} --version`
/// Windows:     `where.exe {name}` then `{name} --version`
pub fn check_tool(name: &str) -> Option<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        macos::check_tool(name)
    }
    #[cfg(target_os = "windows")]
    {
        windows::check_tool(name)
    }
    #[cfg(target_os = "linux")]
    {
        linux::check_tool(name)
    }
}

/// Additional directories to search for tools beyond $PATH.
///
/// macOS:   ~/.operon/node/bin, /opt/homebrew/bin, /usr/local/bin
/// Windows: %APPDATA%\npm, %ProgramFiles%\nodejs
/// Linux:   ~/.operon/node/bin, /usr/local/bin, ~/.local/bin
pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::extra_tool_paths()
    }
    #[cfg(target_os = "windows")]
    {
        windows::extra_tool_paths()
    }
    #[cfg(target_os = "linux")]
    {
        linux::extra_tool_paths()
    }
}

/// Build an augmented PATH string that includes platform-specific tool locations.
pub fn augmented_path() -> String {
    let extra: Vec<String> = extra_tool_paths()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let current = std::env::var("PATH").unwrap_or_default();
    let sep = path_separator();
    format!("{}{}{}", extra.join(&sep.to_string()), sep, current)
}

// ─── Installation ────────────────────────────────────────────────

/// Platform-specific dependency check for the setup wizard.
pub fn check_dependencies() -> crate::commands::claude::DependencyStatus {
    #[cfg(target_os = "macos")]
    {
        macos::check_dependencies()
    }
    #[cfg(target_os = "windows")]
    {
        windows::check_dependencies()
    }
    #[cfg(target_os = "linux")]
    {
        linux::check_dependencies()
    }
}

/// Install Node.js using the best method for this platform.
pub fn install_node_platform() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::install_node_platform()
    }
    #[cfg(target_os = "windows")]
    {
        windows::install_node_platform()
    }
    #[cfg(target_os = "linux")]
    {
        linux::install_node_platform()
    }
}

/// Install Claude Code.
pub fn install_claude_platform() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::install_claude_platform()
    }
    #[cfg(target_os = "windows")]
    {
        windows::install_claude_platform()
    }
    #[cfg(target_os = "linux")]
    {
        linux::install_claude_platform()
    }
}

/// Refresh the process PATH after installers modify system PATH.
/// On Windows, reads from the registry. On macOS/Linux, no-op (shell inheritance handles it).
pub fn refresh_path() {
    #[cfg(target_os = "windows")]
    {
        windows::refresh_path_from_registry();
    }
    #[cfg(not(target_os = "windows"))]
    { /* macOS/Linux: login shell re-reads PATH automatically */ }
}

/// Install Git for Windows (Windows only, no-op on other platforms).
/// Claude Code on Windows requires Git Bash to function.
pub fn install_git_platform() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::install_git()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    } // Git/bash always available on macOS/Linux
}

/// Persist CLAUDE_CODE_GIT_BASH_PATH as a user-level environment variable.
/// On Windows, uses `setx` so it survives reboots and is visible to all processes.
/// On other platforms, no-op (native bash is always available).
pub fn persist_git_bash_env() {
    #[cfg(target_os = "windows")]
    {
        windows::persist_git_bash_env();
    }
    #[cfg(not(target_os = "windows"))]
    { /* no-op */ }
}

/// Find the Git Bash executable path (Windows only).
/// Returns the path to bash.exe if found, None otherwise.
/// On macOS/Linux, returns None (not needed — native bash is used).
pub fn find_git_bash_path() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        windows::find_git_bash()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Install Xcode CLI tools (macOS only, no-op on other platforms).
pub fn install_xcode_cli_platform() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::install_xcode_cli()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// Install Homebrew (macOS only, no-op on other platforms).
pub fn install_homebrew_platform() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        macos::install_homebrew_silent()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Homebrew is not applicable on this platform".to_string())
    }
}

/// Find the platform package manager binary path (brew/winget/apt).
pub fn find_package_manager() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::find_brew()
    }
    #[cfg(target_os = "windows")]
    {
        windows::find_winget()
    }
    #[cfg(target_os = "linux")]
    {
        linux::find_apt()
    }
}

// ─── SSH ─────────────────────────────────────────────────────────

/// Returns SSH connection multiplexing arguments.
///
/// macOS/Linux: ControlMaster=auto ControlPath=... ControlPersist=4h
/// Windows:     empty (use ssh-agent service instead)
pub fn ssh_mux_args(_host: &str, _port: u16, _user: &str, _as_master: bool) -> String {
    #[cfg(target_os = "macos")]
    {
        macos::ssh_mux_args(_host, _port, _user)
    }
    #[cfg(target_os = "windows")]
    {
        String::new()
    } // ControlMaster not supported on Windows
    #[cfg(target_os = "linux")]
    {
        linux::ssh_mux_args(_host, _port, _user)
    }
}

/// Check if an SSH multiplexed connection is alive.
pub fn ssh_mux_check(_host: &str, _port: u16, _user: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::ssh_mux_check(_host, _port, _user)
    }
    #[cfg(target_os = "windows")]
    {
        false
    }
    #[cfg(target_os = "linux")]
    {
        linux::ssh_mux_check(_host, _port, _user)
    }
}

/// Return the ControlMaster socket path for a given connection.
pub fn ssh_socket_path(host: &str, port: u16, user: &str) -> std::path::PathBuf {
    ssh_sockets_dir().join(format!("ctrl_{}_{}_{}", host, port, user))
}

// ─── Browser & OS Integration ────────────────────────────────────

/// Open a URL in the user's default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::open_url(url)
    }
    #[cfg(target_os = "windows")]
    {
        windows::open_url(url)
    }
    #[cfg(target_os = "linux")]
    {
        linux::open_url(url)
    }
}

/// Open a terminal emulator with a command running in it.
/// Used as a fallback when in-app installation fails.
pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::open_terminal_with_command(command)
    }
    #[cfg(target_os = "windows")]
    {
        windows::open_terminal_with_command(command)
    }
    #[cfg(target_os = "linux")]
    {
        linux::open_terminal_with_command(command)
    }
}

// ─── Python ─────────────────────────────────────────────────────

/// The Python executable name for this platform.
/// macOS/Linux: "python3"
/// Windows:     "python" (Microsoft Store or installer)
pub fn python_command() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "python"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "python3"
    }
}

/// Find the Python executable path.
pub fn find_python() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        windows::find_python()
    }
    #[cfg(target_os = "macos")]
    {
        check_tool("python3")
            .map(|(p, _)| p)
            .or_else(|| check_tool("python").map(|(p, _)| p))
    }
    #[cfg(target_os = "linux")]
    {
        check_tool("python3")
            .map(|(p, _)| p)
            .or_else(|| check_tool("python").map(|(p, _)| p))
    }
}

/// Install Python (Windows only via winget; macOS/Linux assume system Python or brew/apt).
pub fn install_python_platform() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::install_python()
    }
    #[cfg(target_os = "macos")]
    {
        // Try Homebrew
        if let Some(brew) = macos::find_brew() {
            let output = std::process::Command::new(&brew)
                .args(["install", "python@3.12"])
                .output();
            if let Ok(o) = output {
                if o.status.success() {
                    return Ok(());
                }
            }
        }
        Err("Python could not be installed. Install via: brew install python@3.12".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        let has_sudo = std::process::Command::new("sudo")
            .args(["-n", "true"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if has_sudo {
            let result = shell_exec("sudo apt-get install -y python3 python3-pip").output();
            if let Ok(o) = result {
                if o.status.success() {
                    return Ok(());
                }
            }
        }
        Err(
            "Python could not be installed. Install via: sudo apt install python3 python3-pip"
                .to_string(),
        )
    }
}

// ─── OpenSSH ────────────────────────────────────────────────────

/// Check if OpenSSH client is available.
pub fn has_openssh() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::has_openssh()
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    } // Always available on macOS/Linux
}

/// Install OpenSSH client (Windows only; always present on macOS/Linux).
pub fn install_openssh_platform() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::install_openssh()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

// ─── uv / uvx ──────────────────────────────────────────────────

/// Check if uv/uvx is installed.
pub fn has_uv() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::has_uv()
    }
    #[cfg(not(target_os = "windows"))]
    {
        check_tool("uvx").is_some() || check_tool("uv").is_some()
    }
}

/// Install uv (the Python package manager that provides uvx).
pub fn install_uv_platform() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::install_uv()
    }
    #[cfg(target_os = "macos")]
    {
        // Strategy 1: Homebrew
        if let Some(brew) = macos::find_brew() {
            let output = std::process::Command::new(&brew)
                .args(["install", "uv"])
                .output();
            if let Ok(o) = output {
                if o.status.success() {
                    return Ok(());
                }
            }
        }
        // Strategy 2: curl installer
        let result = shell_exec("curl -LsSf https://astral.sh/uv/install.sh | sh").output();
        if let Ok(o) = result {
            if o.status.success() {
                return Ok(());
            }
        }
        Err("uv could not be installed. Install from https://docs.astral.sh/uv/".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        // Strategy 1: curl installer
        let result = shell_exec("curl -LsSf https://astral.sh/uv/install.sh | sh").output();
        if let Ok(o) = result {
            if o.status.success() {
                return Ok(());
            }
        }
        Err("uv could not be installed. Install from https://docs.astral.sh/uv/".to_string())
    }
}

// ─── reportlab ─────────────────────────────────────────────────

/// Check if reportlab (Python PDF library) is installed.
pub fn has_reportlab() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::has_reportlab()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(python_command())
            .args(["-c", "import reportlab"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Install reportlab via pip.
pub fn install_reportlab_platform() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::install_reportlab()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let py = python_command();
        // Strategy 1: --user install (macOS Homebrew Python)
        if let Ok(o) = std::process::Command::new(py)
            .args(["-m", "pip", "install", "reportlab", "--user", "--quiet"])
            .output()
        {
            if o.status.success() {
                return Ok(());
            }
        }
        // Strategy 2: --break-system-packages (Linux)
        if let Ok(o) = std::process::Command::new(py)
            .args([
                "-m",
                "pip",
                "install",
                "reportlab",
                "--quiet",
                "--break-system-packages",
            ])
            .output()
        {
            if o.status.success() {
                return Ok(());
            }
        }
        // Strategy 3: pip3 directly
        if let Ok(o) = std::process::Command::new("pip3")
            .args(["install", "reportlab", "--user", "--quiet"])
            .output()
        {
            if o.status.success() {
                return Ok(());
            }
        }
        Err("reportlab could not be installed. Run: pip3 install reportlab".to_string())
    }
}

// ─── Capabilities (feature flags) ────────────────────────────────

/// Whether native dictation is available on this platform.
pub fn supports_dictation() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(target_os = "windows")]
    {
        false
    } // Could be true in the future via SAPI
    #[cfg(target_os = "linux")]
    {
        false
    }
}

/// Whether SSH ControlMaster multiplexing is supported.
pub fn supports_ssh_mux() -> bool {
    #[cfg(target_os = "windows")]
    {
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

/// Whether Xcode CLI tools are relevant on this platform.
pub fn requires_xcode() -> bool {
    cfg!(target_os = "macos")
}

// ─── Menu ────────────────────────────────────────────────────────

/// Build the native app menu bar.
pub fn build_menu(
    app: &tauri::App,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        macos::build_menu(app)
    }
    #[cfg(target_os = "windows")]
    {
        windows::build_menu(app)
    }
    #[cfg(target_os = "linux")]
    {
        linux::build_menu(app)
    }
}

// ─── Dictation ───────────────────────────────────────────────────

/// Start native speech recognition. Platform-specific.
pub fn start_dictation_platform(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::start_dictation(app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Dictation is not supported on this platform.".to_string())
    }
}

// ─── File System Helpers ─────────────────────────────────────────

/// Check if a file is hidden according to OS conventions.
pub fn is_hidden(path: &std::path::Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::is_hidden(path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'))
    }
}

// ─── Path Helpers ────────────────────────────────────────────────

/// PATH environment variable separator.
/// macOS/Linux: ':'
/// Windows:     ';'
pub fn path_separator() -> char {
    #[cfg(target_os = "windows")]
    {
        ';'
    }
    #[cfg(not(target_os = "windows"))]
    {
        ':'
    }
}

// ─── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_exec_runs_echo() {
        let output = shell_exec("echo hello").output().unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[test]
    fn default_shell_is_not_empty() {
        let shell = default_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn data_dir_is_writable() {
        let dir = data_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let test_file = dir.join("_test_write");
        std::fs::write(&test_file, "test").unwrap();
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn home_dir_exists() {
        let home = home_dir().expect("home_dir should return Some");
        assert!(home.exists());
    }

    #[test]
    fn extra_tool_paths_are_absolute() {
        for path in extra_tool_paths() {
            assert!(path.is_absolute(), "Path should be absolute: {:?}", path);
        }
    }
}
