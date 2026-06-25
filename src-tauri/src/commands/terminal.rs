use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub struct TerminalHandle {
    #[allow(dead_code)]
    pub id: String,
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub child: Arc<Mutex<Box<dyn Child + Send>>>,
}

pub struct TerminalManager {
    pub terminals: Mutex<HashMap<String, TerminalHandle>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            terminals: Mutex::new(HashMap::new()),
        }
    }
}

/// POSIX single-quote a string so it survives a `bash -c "<...>"` wrapper as
/// exactly one argument. Used on Windows, where SSH is routed through Git Bash:
/// every arg — including the remote tmux command — is quoted so the local bash
/// hands it to `ssh` verbatim instead of re-interpreting `&&`, `>`, `$VAR`.
#[cfg(target_os = "windows")]
fn sh_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Drop ControlMaster/ControlPath/ControlPersist `-o` option pairs from an SSH
/// argument list. These rely on Unix-domain sockets, which Windows OpenSSH does
/// not support — passing them to `ssh.exe` produces connection errors/warnings.
/// Used by BOTH the Git Bash and the direct ssh.exe paths so they can't drift.
#[cfg(target_os = "windows")]
fn strip_mux_opts(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-o" && i + 1 < args.len() {
            let next = &args[i + 1];
            if next.starts_with("ControlMaster=")
                || next.starts_with("ControlPath=")
                || next.starts_with("ControlPersist=")
            {
                i += 2;
                continue;
            }
        }
        out.push(args[i].clone());
        i += 1;
    }
    out
}

#[tauri::command]
pub async fn spawn_terminal(
    state: tauri::State<'_, TerminalManager>,
    app: tauri::AppHandle,
    terminal_id: String,
    ssh_args: Option<Vec<String>>,
) -> Result<(), String> {
    // Guard: if this terminal already exists, skip (prevents React StrictMode double-spawn)
    {
        let terminals = state.terminals.lock().map_err(|e| e.to_string())?;
        if terminals.contains_key(&terminal_id) {
            return Ok(());
        }
    }

    let pty_system = native_pty_system();

    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = pty_system.openpty(size).map_err(|e| e.to_string())?;

    // Detect user's shell via platform abstraction
    let shell = crate::platform::default_shell();

    let mut cmd = if let Some(args) = &ssh_args {
        #[cfg(target_os = "windows")]
        {
            // On Windows, route SSH through Git Bash to avoid the ConPTY
            // stall/deadlock bug with interactive SSH. Strip ControlMaster
            // options (no Unix-socket support on Windows), then single-quote
            // every argument so `bash -c` passes each one to `ssh` verbatim —
            // including the remote tmux command, whose `&&`/`||`/`>`/`$SHELL`
            // must be interpreted by the REMOTE shell, not the local bash.
            let stripped = strip_mux_opts(args);
            if let Some(bash_path) = crate::platform::find_git_bash_path() {
                let clean_args: Vec<String> = stripped.iter().map(|a| sh_single_quote(a)).collect();
                let ssh_cmd = format!("ssh -t {}", clean_args.join(" "));
                let mut c = CommandBuilder::new(&bash_path);
                c.arg("-c");
                c.arg(&ssh_cmd);
                c
            } else {
                // Fallback: direct ssh.exe (ConPTY path). Strip ControlMaster
                // options here too — Windows OpenSSH has no Unix-socket muxing.
                let mut c = CommandBuilder::new("ssh.exe");
                c.arg("-t");
                for arg in &stripped {
                    c.arg(arg);
                }
                c
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Spawn SSH directly as the PTY process — no shell wrapper.
            // SSH becomes the root process. -t forces TTY allocation.
            let mut c = CommandBuilder::new("ssh");
            c.arg("-t");
            for arg in args {
                c.arg(arg);
            }
            c
        }
    } else {
        CommandBuilder::new(&shell)
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Share Operon's ssh-agent with the terminal so a passphrase-protected key
    // already unlocked at connect time authenticates without re-prompting.
    if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
        if !sock.is_empty() {
            cmd.env("SSH_AUTH_SOCK", sock);
        }
    }
    // Tell macOS zsh to source /etc/zshrc_Apple_Terminal which emits OSC 7
    // (current working directory) after every command — enables terminal→explorer sync.
    #[cfg(target_os = "macos")]
    cmd.env("TERM_PROGRAM", "Apple_Terminal");

    // Set working directory to home
    if let Some(home) = crate::platform::home_dir() {
        cmd.cwd(&home);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    // Get reader and writer from master
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Store handle
    let handle = TerminalHandle {
        id: terminal_id.clone(),
        master: Arc::new(Mutex::new(pair.master)),
        writer: Arc::new(Mutex::new(writer)),
        child: Arc::new(Mutex::new(child)),
    };

    // Keep a clone of the child Arc for the reader thread so it can wait()
    // on the process after EOF and surface the real exit code.
    let child_for_reader = handle.child.clone();

    state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .insert(terminal_id.clone(), handle);

    // Spawn reader thread (std::thread, NOT tokio — portable-pty Read is synchronous)
    let event_name = format!("pty-output-{}", terminal_id);
    let app_handle = app.clone();
    let tid = terminal_id.clone();

    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = vec![0u8; 8192];

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let output = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_handle.emit(&event_name, serde_json::json!({ "output": output }));
                }
                Err(_) => break,
            }
        }

        // Process exited — wait() to reap and capture the real exit code so the
        // frontend can distinguish ssh auth failure (255) from a clean logout (0).
        // Holds the child mutex briefly; safe because we never .await here.
        let exit_code: Option<i32> = match child_for_reader.lock() {
            Ok(mut child) => match child.wait() {
                Ok(status) => status.exit_code().try_into().ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };

        let _ = app_handle.emit(
            &format!("pty-exit-{}", tid),
            serde_json::json!({ "exit_code": exit_code }),
        );
    });

    Ok(())
}

#[tauri::command]
pub async fn write_terminal(
    state: tauri::State<'_, TerminalManager>,
    terminal_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let terminals = state.terminals.lock().map_err(|e| e.to_string())?;
    let handle = terminals
        .get(&terminal_id)
        .ok_or_else(|| format!("Terminal {} not found", terminal_id))?;

    let mut writer = handle.writer.lock().map_err(|e| e.to_string())?;
    writer.write_all(&data).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn resize_terminal(
    state: tauri::State<'_, TerminalManager>,
    terminal_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let terminals = state.terminals.lock().map_err(|e| e.to_string())?;
    let handle = terminals
        .get(&terminal_id)
        .ok_or_else(|| format!("Terminal {} not found", terminal_id))?;

    let master = handle.master.lock().map_err(|e| e.to_string())?;
    master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get the current working directory of a terminal's shell process.
/// Uses platform-specific methods to read the CWD from the child PID.
/// Only works for local terminals — SSH terminals return an error.
#[tauri::command]
pub async fn get_terminal_cwd(
    state: tauri::State<'_, TerminalManager>,
    terminal_id: String,
) -> Result<String, String> {
    let terminals = state.terminals.lock().map_err(|e| e.to_string())?;
    let handle = terminals
        .get(&terminal_id)
        .ok_or_else(|| format!("Terminal {} not found", terminal_id))?;

    let child = handle.child.lock().map_err(|e| e.to_string())?;
    let pid = child
        .process_id()
        .ok_or_else(|| "Could not get process ID (process may have exited)".to_string())?;

    get_cwd_of_pid(pid)
}

/// Read the current working directory of a process by PID.
fn get_cwd_of_pid(pid: u32) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // Use the absolute path: under launchd (app launched from Finder) PATH is
        // minimal and lacks /usr/sbin, so bare `lsof` silently fails. Fall back to
        // the bare name only if /usr/sbin/lsof doesn't exist.
        let lsof_bin = if std::path::Path::new("/usr/sbin/lsof").exists() {
            "/usr/sbin/lsof"
        } else {
            "lsof"
        };
        let output = std::process::Command::new(lsof_bin)
            .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
            .output()
            .map_err(|e| format!("lsof failed: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix('n') {
                if !path.is_empty() {
                    return Ok(path.to_string());
                }
            }
        }
        Err("Could not determine CWD from lsof".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{}/cwd", pid))
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("Failed to read /proc/{}/cwd: {}", pid, e))
    }
    #[cfg(target_os = "windows")]
    {
        // CWD sync is disabled on Windows. Win32_Process exposes no working-directory
        // field, so the only available heuristic was the directory of the child's
        // ExecutablePath — which reports the shell's install location (e.g.
        // C:\Program Files\Git\cmd) rather than the user's project dir, syncing the
        // explorer to the wrong path. Returning Err leaves the terminal↔explorer sync
        // inert here. Revisit via OSC 7 shell-integration to report CWD reliably.
        let _ = pid;
        Err("Terminal CWD sync is not supported on Windows".to_string())
    }
}

#[tauri::command]
pub async fn kill_terminal(
    state: tauri::State<'_, TerminalManager>,
    terminal_id: String,
) -> Result<(), String> {
    let mut terminals = state.terminals.lock().map_err(|e| e.to_string())?;

    if let Some(handle) = terminals.remove(&terminal_id) {
        if let Ok(mut child) = handle.child.lock() {
            let _ = child.kill();
        }
    }

    Ok(())
}
