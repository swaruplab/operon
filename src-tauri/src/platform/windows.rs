//! Windows platform implementations.

use crate::commands::claude::DependencyStatus;
use std::os::windows::process::CommandExt;

/// Windows flag to suppress console window creation for child processes.
const CREATE_NO_WINDOW: u32 = 0x08000000;

// ─── Shell Execution ─────────────────────────────────────────────

/// Translate common Unix shell idioms so the command works under cmd.exe.
/// Only used by the cmd.exe fallback when Git Bash cannot be found.
fn fixup_for_cmd(command: &str) -> String {
    // /dev/null → nul (Windows null device)
    command.replace("/dev/null", "nul")
}

/// Resolve Git Bash once and cache the result. `find_git_bash()` does
/// filesystem probes and may spawn `where.exe`, so it should not run on every
/// `shell_exec`. Only a *successful* lookup is cached — a probe that happens
/// before Git is installed (first-launch setup) must not poison the cache.
fn git_bash_cached() -> Option<String> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    if let Some(p) = CACHE.get() {
        return Some(p.clone());
    }
    let found = find_git_bash();
    if let Some(ref p) = found {
        let _ = CACHE.set(p.clone());
    }
    found
}

/// Build the `(program, args)` pair for running a POSIX command string.
///
/// Every command string in Operon is POSIX/bash — single quotes, pipes,
/// `which`, coreutils (`base64`, `rm`, `cat`), `2>/dev/null`. cmd.exe cannot
/// parse any of that, so commands run through Git Bash as a login shell
/// (`-l` so `/etc/profile` sets up the full tool PATH). Git Bash is already a
/// hard requirement on Windows; cmd.exe is only a degraded last resort.
fn shell_invocation(command: &str) -> (String, Vec<String>) {
    match git_bash_cached() {
        Some(bash) => (
            bash,
            vec!["-l".to_string(), "-c".to_string(), command.to_string()],
        ),
        None => (
            "cmd.exe".to_string(),
            vec!["/C".to_string(), fixup_for_cmd(command)],
        ),
    }
}

pub fn shell_exec(command: &str) -> std::process::Command {
    let (program, args) = shell_invocation(command);
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    let (program, args) = shell_invocation(command);
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn default_shell() -> String {
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

// ─── Tool Discovery ──────────────────────────────────────────────

pub fn check_tool(name: &str) -> Option<(String, String)> {
    let where_out = std::process::Command::new("where.exe")
        .arg(name)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !where_out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&where_out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    let ver_out = std::process::Command::new(&path)
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&ver_out.stdout).trim().to_string();
    Some((path, version))
}

/// Resolve a CLI command name to a directly-spawnable `(program, leading_args)`.
///
/// `Command::new("foo")` on Windows only finds `foo.exe` and cannot execute a
/// `.cmd`/`.bat` shim at all (CreateProcess rejects batch files — and npm-
/// installed CLIs like language servers ship exactly such shims). This resolves
/// the name via `where.exe`, which DOES see PATHEXT shims, and wraps a batch
/// shim as `("cmd.exe", ["/c", "<resolved>"])` so it can be spawned.
pub fn spawn_resolve(name: &str) -> (String, Vec<String>) {
    // An explicit path that already exists: use it directly.
    let p = std::path::Path::new(name);
    let explicit = if p.components().count() > 1 && p.exists() {
        Some(name.to_string())
    } else {
        None
    };
    let resolved = explicit.or_else(|| {
        let out = std::process::Command::new("where.exe")
            .arg(name)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    });
    match resolved {
        Some(path) => {
            let lower = path.to_lowercase();
            if lower.ends_with(".cmd") || lower.ends_with(".bat") {
                ("cmd.exe".to_string(), vec!["/c".to_string(), path])
            } else {
                (path, Vec::new())
            }
        }
        // Not found — return the bare name so the caller's spawn fails clearly.
        None => (name.to_string(), Vec::new()),
    }
}

pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let appdata = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join("AppData").join("Roaming"));
    let localappdata = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join("AppData").join("Local"));
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    vec![
        // Operon's per-user portable Node: the Windows zip puts node.exe /
        // npm.cmd at the ROOT of the extracted dir (no bin/ subdir like Unix).
        super::operon_node_dir(),
        super::operon_node_dir().join("bin"),
        // Operon's per-user portable GitHub CLI: gh.exe lives under bin/.
        super::operon_gh_dir().join("bin"),
        // Operon's per-user PortableGit (no-admin Git). bash/git live in bin/cmd;
        // ssh and friends in usr/bin.
        super::operon_git_dir().join("cmd"),
        super::operon_git_dir().join("bin"),
        super::operon_git_dir().join("usr").join("bin"),
        appdata.join("npm"),
        std::path::PathBuf::from(r"C:\Program Files\nodejs"),
        std::path::PathBuf::from(r"C:\Program Files\Git\bin"),
        std::path::PathBuf::from(r"C:\Program Files\Git\cmd"),
        home.join(".claude\\local\\bin"),
        // OpenSSH client binaries (ssh-keygen, ssh-add) live in this System32
        // SUBDIR, which isn't always on a GUI process's inherited PATH.
        std::path::PathBuf::from(format!(r"{}\System32\OpenSSH", sysroot)),
        // winget drops package shims here (uv, uvx, gh, …). A just-installed
        // winget package is reachable here even before the registry PATH change
        // propagates to this process — so detection finds it without a restart.
        localappdata.join("Microsoft").join("WinGet").join("Links"),
        // uv's standalone-installer default location.
        home.join(".local").join("bin"),
    ]
}

/// Resolve `ssh-keygen.exe` to a full path. A bare `Command::new("ssh-keygen")`
/// relies on the inherited PATH, which often omits `System32\OpenSSH` (a subdir,
/// not System32 itself) when the app is launched from Explorer — so key
/// generation fails with "program not found" even though `ssh.exe` was detected
/// (detection uses `where.exe`, which searches differently).
pub fn find_ssh_keygen() -> Option<String> {
    // 1. where.exe (PATHEXT-aware) — finds it whenever the dir is on PATH.
    let (resolved, _) = spawn_resolve("ssh-keygen");
    let p = std::path::Path::new(&resolved);
    if p.is_absolute() && p.exists() {
        return Some(resolved);
    }
    // 2. Windows OpenSSH — the sibling of the ssh.exe the app already found.
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let win_openssh = format!(r"{}\System32\OpenSSH\ssh-keygen.exe", sysroot);
    if std::path::Path::new(&win_openssh).exists() {
        return Some(win_openssh);
    }
    // 3. Git Bash's usr\bin (derive the Git root from bash.exe: <git>\bin\bash.exe).
    if let Some(bash) = find_git_bash() {
        if let Some(git_root) = std::path::Path::new(&bash)
            .parent()
            .and_then(|p| p.parent())
        {
            let candidate = git_root.join("usr").join("bin").join("ssh-keygen.exe");
            if candidate.exists() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Refresh the process's PATH environment variable from the Windows registry.
///
/// After winget/msi installs, the system PATH is updated but our running
/// process still has the old PATH. This reads the current User + Machine
/// PATH values from the registry and updates the process environment.
/// Expand `%VAR%` references using the current process environment. The
/// registry stores PATH as REG_EXPAND_SZ (e.g. `%SystemRoot%\system32`), and
/// `reg query` returns it UNEXPANDED — so without this, those entries (and a
/// winget Python install under `%LOCALAPPDATA%\...`) are invalid in the process
/// PATH and tools there aren't found. Unknown vars are left literal.
fn expand_env_vars(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            match std::env::var(name) {
                Ok(val) => out.push_str(&val),
                Err(_) => {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
            }
            rest = &after[end + 1..];
        } else {
            out.push('%');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub fn refresh_path_from_registry() {
    let machine_path = read_registry_path(
        "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        "Path",
    )
    .map(|p| expand_env_vars(&p));
    let user_path = read_registry_path("HKCU\\Environment", "Path").map(|p| expand_env_vars(&p));

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
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // reg query output: "    Path    REG_EXPAND_SZ    C:\...;C:\..."
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(value)
            || trimmed.contains("REG_EXPAND_SZ")
            || trimmed.contains("REG_SZ")
        {
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
        .creation_flags(CREATE_NO_WINDOW)
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
    let system32 = std::env::var("SYSTEMROOT").unwrap_or_else(|_| r"C:\Windows".to_string());
    let ps_path = format!(
        r"{}\System32\WindowsPowerShell\v1.0\powershell.exe",
        system32
    );

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
    cmd.spawn().map_err(|e| {
        format!(
            "Failed to open any terminal (tried wt, PowerShell, cmd): {}",
            e
        )
    })?;
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
        // Operon's own per-user PortableGit (no-admin install) — checked first.
        super::operon_git_dir()
            .join("bin")
            .join("bash.exe")
            .to_string_lossy()
            .to_string(),
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
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
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
            .lines()
            .next()?
            .trim()
            .to_string();
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

/// Install Git for Windows per-user with NO admin.
///
/// Uses the **PortableGit** self-extracting archive (a 7-Zip SFX), not the
/// standard installer: the installer can request UAC for an all-users install,
/// which locked-down lab accounts cannot grant. PortableGit only *extracts* —
/// no elevation, no wizard, no PATH write (extra_tool_paths() surfaces it). We
/// unpack it into operon_git_dir(), which find_git_bash() checks first.
///
/// Returns Ok(()) when Git is actually installed (unlike the old GUI path).
/// Returns Err("BROWSER_OPENED") if the download fails, so the caller shows the
/// manual-download fallback.
pub fn install_git() -> Result<(), String> {
    let arch = if cfg!(target_arch = "x86_64") {
        "64-bit"
    } else {
        "arm64"
    };
    let version = "2.54.0";
    let url = format!(
        "https://github.com/git-for-windows/git/releases/download/v{v}.windows.1/PortableGit-{v}-{a}.7z.exe",
        v = version,
        a = arch
    );
    let sfx = super::temp_dir().join("PortableGit.7z.exe");
    let dest = super::operon_git_dir();

    eprintln!("[Git] Downloading PortableGit {} ...", url);
    if download_file(&url, &sfx).is_err() {
        eprintln!("[Git] PortableGit download failed, opening browser");
        let _ = open_url("https://git-scm.com/downloads/win");
        return Err("BROWSER_OPENED".to_string());
    }

    if dest.exists() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    // 7-Zip SFX flags: `-o<dir>` sets the output dir, `-y` assumes yes — a
    // silent, non-elevated extraction. `.status()` waits for it to finish.
    eprintln!("[Git] Extracting PortableGit to {} ...", dest.display());
    let status = std::process::Command::new(&sfx)
        .arg(format!("-o{}", dest.display()))
        .arg("-y")
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("PortableGit extraction failed to launch: {}", e))?;
    let _ = std::fs::remove_file(&sfx);

    if !status.success() {
        return Err("PortableGit extraction did not complete.".to_string());
    }
    if !dest.join("bin").join("bash.exe").exists() {
        return Err("Git Bash not found after extraction.".to_string());
    }
    persist_git_bash_env();
    Ok(())
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
            .creation_flags(CREATE_NO_WINDOW)
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

/// Build a headless `winget install <id>` command. Resolves winget via
/// `where.exe` (the App Installer execution-alias dir isn't always on a GUI
/// process's PATH) and disables interactivity + installer UI: under
/// CREATE_NO_WINDOW there is no console/TTY for winget's prompts to render to,
/// so without `--disable-interactivity`/`--silent` winget aborts and the
/// install "fails" — which is exactly what users saw for Node/Python/uv/gh.
pub(crate) fn winget_install_cmd(id: &str) -> std::process::Command {
    let winget = find_winget().unwrap_or_else(|| "winget".to_string());
    let mut c = std::process::Command::new(winget);
    c.args([
        "install",
        "--id",
        id,
        "-e",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--disable-interactivity",
        "--silent",
    ]);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Download `url` to `dest` with NO admin rights: certutil first (built into
/// every Windows, no PowerShell needed), PowerShell Invoke-WebRequest fallback.
fn download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let dest_str = dest.to_string_lossy().to_string();

    let certutil = std::process::Command::new("certutil")
        .args(["-urlcache", "-split", "-f", url, &dest_str])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(o) = certutil {
        if o.status.success() && dest.exists() {
            return Ok(());
        }
    }

    let ps = format!(
        "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
        url, dest_str
    );
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Download failed to launch: {}", e))?;
    if out.status.success() && dest.exists() {
        Ok(())
    } else {
        Err(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// Extract a .zip into `dest`, stripping the single top-level directory the
/// archive wraps everything in (like `tar --strip-components=1`). NO admin.
fn extract_zip_stripped(zip: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }
    std::fs::create_dir_all(dest)
        .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    // Strategy 1: bsdtar (Windows 10 1803+ ships tar.exe) handles zip +
    // --strip-components in one shot, mirroring the macOS/Linux path.
    let tar = std::process::Command::new("tar")
        .arg("-xf")
        .arg(zip)
        .args(["--strip-components=1", "-C"])
        .arg(dest)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(o) = tar {
        if o.status.success() {
            return Ok(());
        }
    }

    // Strategy 2: PowerShell Expand-Archive into a staging dir, then promote the
    // single wrapper folder's contents up into `dest`.
    let staging = super::temp_dir().join("operon_zip_stage");
    let _ = std::fs::remove_dir_all(&staging);
    let ps = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        zip.display(),
        staging.display()
    );
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Expand-Archive failed to launch: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "Failed to extract archive: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut entries: Vec<_> = std::fs::read_dir(&staging)
        .map_err(|e| format!("read staging dir: {}", e))?
        .filter_map(|e| e.ok())
        .collect();
    let root = if entries.len() == 1 && entries[0].path().is_dir() {
        entries.remove(0).path()
    } else {
        staging.clone()
    };
    for entry in std::fs::read_dir(&root).map_err(|e| format!("read extracted root: {}", e))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = dest.join(entry.file_name());
        std::fs::rename(entry.path(), &target)
            .map_err(|e| format!("move {}: {}", entry.path().display(), e))?;
    }
    let _ = std::fs::remove_dir_all(&staging);
    Ok(())
}

pub fn install_node_platform() -> Result<(), String> {
    // Per-user PORTABLE install — NO admin. The official Node ZIP unpacks
    // node.exe + npm at the root of its wrapper dir; extracting with
    // --strip-components=1 lands them directly in operon_node_dir(), which
    // extra_tool_paths() puts on Operon's spawned PATH. (The winget/MSI path
    // needed admin, so it's gone — locked-down accounts can't elevate.)
    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "arm64"
    };
    let node_version = "v22.14.0";
    let url = format!(
        "https://nodejs.org/dist/{}/node-{}-win-{}.zip",
        node_version, node_version, arch
    );
    let dest = super::operon_node_dir();
    let tmp_zip = super::temp_dir().join("operon_node.zip");

    eprintln!("[Node] Downloading {} ...", url);
    download_file(&url, &tmp_zip)?;

    eprintln!("[Node] Extracting to {} ...", dest.display());
    extract_zip_stripped(&tmp_zip, &dest)?;
    let _ = std::fs::remove_file(&tmp_zip);

    if !dest.join("node.exe").exists() {
        return Err("Node binary not found after extraction".to_string());
    }
    refresh_path_from_registry();
    Ok(())
}

/// Install GitHub CLI per-user (PORTABLE zip) — NO admin. The MSI/winget path
/// needs admin; the official release zip doesn't. After --strip-components=1,
/// gh.exe lands at operon_gh_dir()/bin/gh.exe (extra_tool_paths() adds that dir).
pub fn install_gh() -> Result<(), String> {
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else {
        "arm64"
    };
    let version = "2.63.2";
    let url = format!(
        "https://github.com/cli/cli/releases/download/v{v}/gh_{v}_windows_{a}.zip",
        v = version,
        a = arch
    );
    let dest = super::operon_gh_dir();
    let tmp_zip = super::temp_dir().join("operon_gh.zip");

    eprintln!("[gh] Downloading {} ...", url);
    download_file(&url, &tmp_zip)?;

    eprintln!("[gh] Extracting to {} ...", dest.display());
    extract_zip_stripped(&tmp_zip, &dest)?;
    let _ = std::fs::remove_file(&tmp_zip);

    if !dest.join("bin").join("gh.exe").exists() {
        return Err("gh binary not found after extraction".to_string());
    }
    Ok(())
}

pub fn install_claude_platform() -> Result<(), String> {
    // Refresh PATH first so we can find npm/node installed moments ago
    refresh_path_from_registry();

    // Strategy 1: Official curl installer via Git Bash (most reliable)
    if let Some(bash_path) = find_git_bash() {
        let result = std::process::Command::new(&bash_path)
            .args(["-c", "curl -fsSL https://claude.ai/install.sh | bash"])
            .creation_flags(CREATE_NO_WINDOW)
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
            Err(format!(
                "npm install failed: {}. Run: npm install -g @anthropic-ai/claude-code",
                stderr.chars().take(200).collect::<String>()
            ))
        }
        Err(e) => Err(format!(
            "Could not run npm: {}. Ensure Node.js is installed and restart Operon.",
            e
        )),
    }
}

pub fn find_winget() -> Option<String> {
    let out = std::process::Command::new("where.exe")
        .arg("winget")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if out.status.success() {
        Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()?
                .trim()
                .to_string(),
        )
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
    // Per-user install (NO admin): `--scope user` lands Python in
    // %LOCALAPPDATA%\Programs\Python. winget runs as the current (non-elevated)
    // user, whose package source is already initialized — so no elevation and no
    // 0x8a15000f source error from a borrowed admin profile.
    let winget = find_winget().unwrap_or_else(|| "winget".to_string());
    let out = std::process::Command::new(&winget)
        .args([
            "install",
            "--id",
            "Python.Python.3.12",
            "-e",
            "--scope",
            "user",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--accept-package-agreements",
            "--disable-interactivity",
            "--silent",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(o) = out {
        let out_text = format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        if o.status.success() || out_text.contains("already installed") {
            refresh_path_from_registry();
            return Ok(());
        }
    }

    Err("Python could not be installed automatically. Please install from https://python.org/downloads (choose 'Install for me only') and restart Operon.".to_string())
}

// ─── OpenSSH ────────────────────────────────────────────────────

/// Check if an OpenSSH client is available — either the Windows native one on
/// PATH, or the ssh.exe bundled inside Git for Windows (`<git>\usr\bin`).
pub fn has_openssh() -> bool {
    if check_tool("ssh").is_some() {
        return true;
    }
    // Git for Windows bundles ssh.exe under <git>\usr\bin (derive <git> from
    // <git>\bin\bash.exe). This is what lets us avoid the admin-only
    // Add-WindowsCapability on locked-down accounts.
    if let Some(bash) = find_git_bash() {
        if let Some(git_root) = std::path::Path::new(&bash)
            .parent()
            .and_then(|p| p.parent())
        {
            if git_root.join("usr").join("bin").join("ssh.exe").exists() {
                return true;
            }
        }
    }
    false
}

/// Ensure an OpenSSH client is available WITHOUT admin.
///
/// We deliberately do NOT enable the Windows OpenSSH optional feature
/// (`Add-WindowsCapability` needs administrator, which locked-down lab accounts
/// don't have). Git for Windows bundles ssh.exe, and Operon uses that for remote
/// connections, so a present Git install satisfies this. If neither is found we
/// return guidance instead of silently elevating.
pub fn install_openssh() -> Result<(), String> {
    if has_openssh() {
        return Ok(());
    }
    Err("OpenSSH client not found. It ships with Git for Windows (which Operon installs), or enable it via Settings → Apps → Optional Features → OpenSSH Client.".to_string())
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
        .args([
            "-ExecutionPolicy",
            "ByPass",
            "-Command",
            "irm https://astral.sh/uv/install.ps1 | iex",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(o) = ps {
        if o.status.success() {
            return Ok(());
        }
    }

    // Strategy 2: winget
    let winget = winget_install_cmd("astral-sh.uv").output();

    if let Ok(o) = winget {
        let out_text = format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        if o.status.success() || out_text.contains("already installed") {
            return Ok(());
        }
    }

    // Strategy 3: pip install uv (requires Python)
    if find_python().is_some() {
        let pip = shell_exec("pip install uv").output();
        if let Ok(o) = pip {
            if o.status.success() {
                return Ok(());
            }
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
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Install reportlab via pip.
pub fn install_reportlab() -> Result<(), String> {
    let python = find_python().ok_or("Python is not installed — cannot install reportlab.")?;

    // Strategy 1: pip install --user
    if let Ok(o) = std::process::Command::new(&python)
        .args(["-m", "pip", "install", "reportlab", "--user", "--quiet"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        if o.status.success() {
            return Ok(());
        }
    }

    // Strategy 2: pip install (no --user)
    if let Ok(o) = std::process::Command::new(&python)
        .args(["-m", "pip", "install", "reportlab", "--quiet"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        if o.status.success() {
            return Ok(());
        }
    }

    Err("reportlab could not be installed. Run: pip install reportlab".to_string())
}

// ─── File System ─────────────────────────────────────────────────

/// Check if a file is hidden on Windows.
/// Checks both the Windows Hidden file attribute AND dot-prefix (Unix convention
/// used by Git, npm, etc. — common on Windows developer machines).
pub fn is_hidden(path: &std::path::Path) -> bool {
    // Check dot-prefix (Unix convention, common for .git, .env, .gitignore, etc.)
    let dot_hidden = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'));

    if dot_hidden {
        return true;
    }

    // Check Windows FILE_ATTRIBUTE_HIDDEN
    use std::os::windows::fs::MetadataExt;
    if let Ok(meta) = std::fs::metadata(path) {
        meta.file_attributes() & 0x2 != 0
    } else {
        false
    }
}

// ─── Menu ────────────────────────────────────────────────────────

pub fn build_menu(
    app: &tauri::App,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

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

    let help_item = MenuItemBuilder::with_id("open-help", "Operon Help").build(app)?;

    let help_submenu = SubmenuBuilder::new(app, "Help").item(&help_item).build()?;

    let menu = MenuBuilder::new(app)
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&help_submenu)
        .build()?;

    Ok(menu)
}
