use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty, Child};
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
        // Spawn SSH directly as the PTY process — no shell wrapper.
        // SSH becomes the root process. -t forces TTY allocation.
        let mut c = CommandBuilder::new("ssh");
        c.arg("-t"); // Force interactive TTY
        for arg in args {
            c.arg(arg);
        }
        c
    } else {
        CommandBuilder::new(&shell)
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

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
                    let _ = app_handle.emit(
                        &event_name,
                        serde_json::json!({ "output": output }),
                    );
                }
                Err(_) => break,
            }
        }

        // Process exited — notify frontend
        let _ = app_handle.emit(
            &format!("pty-exit-{}", tid),
            serde_json::json!({}),
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
