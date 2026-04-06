//! macOS platform implementations.

use crate::commands::claude::DependencyStatus;

// ─── Shell Execution ─────────────────────────────────────────────

pub fn shell_exec(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = std::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = tokio::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

// ─── Tool Discovery ──────────────────────────────────────────────

pub fn check_tool(name: &str) -> Option<(String, String)> {
    let which = shell_exec(&format!("which {}", name)).output().ok()?;
    if !which.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&which.stdout).trim().to_string();
    let ver_out = shell_exec(&format!("{} --version", name)).output().ok()?;
    let version = String::from_utf8_lossy(&ver_out.stdout).trim().to_string();
    Some((path, version))
}

pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        super::operon_node_dir().join("bin"),
        std::path::PathBuf::from("/opt/homebrew/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
        home.join(".claude/local/bin"),
        home.join(".npm-global/bin"),
    ]
}

// ─── Browser & OS Integration ────────────────────────────────────

pub fn open_url(url: &str) -> Result<(), String> {
    // Write the URL to a temp file and use Python to open it.
    // This avoids all encoding issues with shell args and AppleScript.
    let tmp_dir = std::env::temp_dir();
    let url_file = tmp_dir.join("operon_open_url.txt");

    std::fs::write(&url_file, url).map_err(|e| format!("Failed to write URL file: {}", e))?;

    let python_cmd = format!(
        "import webbrowser; url=open('{}').read().strip(); webbrowser.open(url)",
        url_file.to_string_lossy().replace('\'', "\\'")
    );

    std::process::Command::new("python3")
        .args(["-c", &python_cmd])
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;

    Ok(())
}

pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#,
        command.replace('\\', "\\\\").replace('"', "\\\"")
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("osascript failed: {}", e))?;
    Ok(())
}

// ─── SSH ─────────────────────────────────────────────────────────

pub fn ssh_mux_args(host: &str, port: u16, user: &str) -> String {
    let sock = super::ssh_socket_path(host, port, user);
    // Quote the ControlPath — it lives under ~/Library/Application Support/
    // which has a space that breaks shell parsing if unquoted
    format!(
        " -o ControlMaster=auto -o \"ControlPath={}\" -o ControlPersist=4h",
        sock.display()
    )
}

pub fn ssh_mux_check(host: &str, port: u16, user: &str) -> bool {
    let sock = super::ssh_socket_path(host, port, user);
    let check_cmd = format!(
        "ssh -o \"ControlPath={}\" -O check {}@{} -p {} 2>/dev/null",
        sock.display(),
        user,
        host,
        port
    );
    shell_exec(&check_cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── Installation ────────────────────────────────────────────────

pub fn check_dependencies() -> DependencyStatus {
    let aug_path = super::augmented_path();

    let check_cmd = |cmd: &str| -> Option<std::process::Output> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        std::process::Command::new(&shell)
            .arg("-l")
            .arg("-c")
            .arg(cmd)
            .env("PATH", &aug_path)
            .output()
            .ok()
    };

    // Check Xcode CLI tools
    let xcode = check_cmd("xcode-select -p")
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Check Node.js
    let node_out = check_cmd("node --version");
    let mut node = node_out.as_ref().is_some_and(|o| o.status.success());
    let mut node_version = node_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if !node {
        for p in extra_tool_paths() {
            let node_bin = p.join("node");
            if node_bin.exists() {
                if let Ok(out) = std::process::Command::new(&node_bin)
                    .arg("--version")
                    .output()
                {
                    if out.status.success() {
                        node = true;
                        node_version =
                            Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
                        break;
                    }
                }
            }
        }
    }

    // Check npm
    let npm_out = check_cmd("npm --version");
    let mut npm = npm_out.as_ref().is_some_and(|o| o.status.success());
    let mut npm_version = npm_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if !npm {
        for p in extra_tool_paths() {
            let npm_bin = p.join("npm");
            if npm_bin.exists() {
                if let Ok(out) = std::process::Command::new(&npm_bin)
                    .arg("--version")
                    .output()
                {
                    if out.status.success() {
                        npm = true;
                        npm_version = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
                        break;
                    }
                }
            }
        }
    }

    // Check Claude Code
    let claude_out = check_cmd("claude --version");
    let claude_code = claude_out.as_ref().is_some_and(|o| o.status.success());
    let claude_version = claude_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    DependencyStatus {
        xcode_cli: xcode,
        node,
        node_version,
        npm,
        npm_version,
        claude_code,
        claude_version,
        git_bash: true, // Not required on macOS — bash is always available
    }
}

pub fn install_xcode_cli() -> Result<(), String> {
    let check = shell_exec("xcode-select -p")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if check {
        return Ok(());
    }

    let output = std::process::Command::new("xcode-select")
        .arg("--install")
        .output()
        .map_err(|e| {
            format!("Could not launch Xcode CLI installer: {}. Please run 'xcode-select --install' in Terminal.", e)
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("already installed") || stderr.contains("install requested") {
            return Ok(());
        }
        return Err(format!("Failed to start Xcode CLI install: {}", stderr));
    }
    Ok(())
}

/// Download a Node.js tar.gz, extract to the Operon-managed dir, and add to PATH.
fn install_node_tarball() -> Result<(), String> {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let node_version = "v22.14.0";
    let tarball_url = format!(
        "https://nodejs.org/dist/{}/node-{}-darwin-{}.tar.gz",
        node_version, node_version, arch
    );

    let dest = super::operon_node_dir();
    let tmp_tar = super::temp_dir().join("operon_node.tar.gz");

    eprintln!("[Node] Downloading {} ...", tarball_url);
    let dl = std::process::Command::new("curl")
        .args(["-fSL", "--progress-bar", "-o"])
        .arg(&tmp_tar)
        .arg(&tarball_url)
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if !dl.status.success() {
        let stderr = String::from_utf8_lossy(&dl.stderr);
        return Err(format!("Download failed: {}", stderr));
    }

    if dest.exists() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    eprintln!("[Node] Extracting to {} ...", dest.display());
    let extract = std::process::Command::new("tar")
        .args(["xzf"])
        .arg(&tmp_tar)
        .args(["--strip-components=1", "-C"])
        .arg(&dest)
        .output()
        .map_err(|e| format!("tar failed: {}", e))?;

    if !extract.status.success() {
        return Err(format!(
            "Extract failed: {}",
            String::from_utf8_lossy(&extract.stderr)
        ));
    }
    let _ = std::fs::remove_file(&tmp_tar);

    let node_bin = dest.join("bin").join("node");
    if !node_bin.exists() {
        return Err("Node binary not found after extraction".to_string());
    }

    let check = std::process::Command::new(&node_bin)
        .arg("--version")
        .output();
    match check {
        Ok(o) if o.status.success() => {
            eprintln!(
                "[Node] Installed: {}",
                String::from_utf8_lossy(&o.stdout).trim()
            );
        }
        _ => return Err("Node binary exists but won't run".to_string()),
    }

    // Add to shell profile
    let home = dirs::home_dir().unwrap_or_default();
    let bin_dir = dest.join("bin");
    let path_line = format!("\nexport PATH=\"{}:$PATH\"\n", bin_dir.to_string_lossy());

    for profile_name in &[".zprofile", ".bash_profile"] {
        let profile_path = home.join(profile_name);
        if profile_path.exists() || *profile_name == ".zprofile" {
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

pub fn install_node_platform() -> Result<(), String> {
    // Already installed?
    let has_node = shell_exec("node --version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_node {
        return Ok(());
    }

    // Check Operon-managed install
    if super::operon_node_dir().join("bin").join("node").exists() {
        return Ok(());
    }

    // Try Homebrew
    if let Some(brew) = find_brew() {
        eprintln!("[Node] Trying Homebrew...");
        let output = shell_exec(&format!("{} install node", brew)).output();
        if let Ok(o) = output {
            if o.status.success() {
                return Ok(());
            }
        }
    }

    // Fallback: tarball
    install_node_tarball()
}

pub fn install_claude_platform() -> Result<(), String> {
    let has_claude = shell_exec("claude --version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_claude {
        return Ok(());
    }

    // Method 1: Official installer
    let output = shell_exec("curl -fsSL https://claude.ai/install.sh | bash").output();
    if let Ok(ref o) = output {
        if o.status.success() {
            let check = shell_exec("claude --version").output();
            if check.map(|c| c.status.success()).unwrap_or(false) {
                return Ok(());
            }
            if let Some(home) = dirs::home_dir() {
                if home.join(".claude/local/bin/claude").exists() {
                    return Ok(());
                }
            }
        }
    }

    // Method 2: npm
    if let Some(npm) = find_npm() {
        let result = shell_exec(&format!("{} install -g @anthropic-ai/claude-code", npm)).output();
        if let Ok(o) = result {
            if o.status.success() {
                return Ok(());
            }
        }
    }

    // Method 3: Open Terminal as last resort
    open_terminal_with_command("curl -fsSL https://claude.ai/install.sh | bash")?;
    Ok(())
}

/// Silently install Homebrew.
pub fn install_homebrew_silent() -> Result<String, String> {
    if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        return Ok("/opt/homebrew/bin/brew".to_string());
    }
    if std::path::Path::new("/usr/local/bin/brew").exists() {
        return Ok("/usr/local/bin/brew".to_string());
    }

    let is_arm = cfg!(target_arch = "aarch64");
    let prefix = if is_arm {
        "/opt/homebrew"
    } else {
        "/usr/local"
    };

    let current_user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| {
            String::from_utf8_lossy(
                &std::process::Command::new("id")
                    .arg("-un")
                    .output()
                    .map(|o| o.stdout)
                    .unwrap_or_default(),
            )
            .trim()
            .to_string()
        });

    // Phase 1: Create directories with admin privileges
    let subdirs = [
        "bin",
        "etc",
        "include",
        "lib",
        "sbin",
        "share",
        "var",
        "opt",
        "Cellar",
        "Caskroom",
        "Frameworks",
        "etc/bash_completion.d",
        "lib/pkgconfig",
        "share/aclocal",
        "share/doc",
        "share/info",
        "share/locale",
        "share/man",
        "share/man/man1",
        "share/man/man2",
        "share/man/man3",
        "share/man/man4",
        "share/man/man5",
        "share/man/man6",
        "share/man/man7",
        "share/man/man8",
        "share/zsh",
        "share/zsh/site-functions",
        "var/homebrew",
        "var/homebrew/linked",
        "var/log",
    ];
    let mkdir_list: Vec<String> = subdirs
        .iter()
        .map(|s| format!("{}/{}", prefix, s))
        .collect();

    let admin_script = format!(
        "mkdir -p {} {} && chown -R {}:admin {} && chmod -R 755 {} && chmod go-w {}/share/zsh {}/share/zsh/site-functions",
        prefix, mkdir_list.join(" "), current_user, prefix, prefix, prefix, prefix,
    );
    let osascript_cmd = format!(
        r#"do shell script "{}" with administrator privileges"#,
        admin_script.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let mkdir_result = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&osascript_cmd)
        .output()
        .map_err(|e| format!("osascript failed: {}", e))?;

    if !mkdir_result.status.success() {
        let stderr = String::from_utf8_lossy(&mkdir_result.stderr);
        if stderr.contains("cancel") || stderr.contains("-128") {
            return Err("Password dialog was cancelled.".to_string());
        }
        return Err(format!("Failed to create Homebrew directories: {}", stderr));
    }

    let home = dirs::home_dir().unwrap_or_default();
    let _ = std::fs::create_dir_all(home.join("Library/Caches/Homebrew"));

    // Phase 2: Clone Homebrew repo
    let tmp_clone = format!("{}/homebrew-clone-tmp", super::temp_dir().display());
    let _ = std::fs::remove_dir_all(&tmp_clone);

    let clone_result = std::process::Command::new("git")
        .args([
            "clone",
            "--depth=1",
            "https://github.com/Homebrew/brew",
            &tmp_clone,
        ])
        .output()
        .map_err(|e| format!("git clone failed: {}", e))?;

    if !clone_result.status.success() {
        let _ = std::fs::remove_dir_all(&tmp_clone);
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&clone_result.stderr)
        ));
    }

    let rsync_result = std::process::Command::new("rsync")
        .args(["-a", &format!("{}/", tmp_clone), &format!("{}/", prefix)])
        .output();

    if rsync_result.map(|o| !o.status.success()).unwrap_or(true) {
        let _ = std::process::Command::new("/bin/bash")
            .args(["-c", &format!("cp -a {}/* {}/", tmp_clone, prefix)])
            .output();
        let _ = std::process::Command::new("/bin/bash")
            .args(["-c", &format!("cp -a {}/.[!.]* {}/", tmp_clone, prefix)])
            .output();
    }
    let _ = std::fs::remove_dir_all(&tmp_clone);

    let brew_bin = format!("{}/bin/brew", prefix);
    if !std::path::Path::new(&brew_bin).exists() {
        return Err(format!("brew binary not found at {} after clone", brew_bin));
    }

    let _ = std::process::Command::new(&brew_bin)
        .args(["update", "--force", "--quiet"])
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .output();

    // Add to shell profile
    let zprofile = home.join(".zprofile");
    let shellenv_line = format!("\neval \"$({} shellenv)\"\n", brew_bin);
    if let Ok(existing) = std::fs::read_to_string(&zprofile) {
        if !existing.contains("brew shellenv") {
            let _ = std::fs::write(&zprofile, format!("{}{}", existing, shellenv_line));
        }
    } else {
        let _ = std::fs::write(&zprofile, &shellenv_line);
    }

    Ok(brew_bin)
}

pub fn find_brew() -> Option<String> {
    if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        Some("/opt/homebrew/bin/brew".to_string())
    } else if std::path::Path::new("/usr/local/bin/brew").exists() {
        Some("/usr/local/bin/brew".to_string())
    } else {
        None
    }
}

fn find_npm() -> Option<String> {
    let operon_npm = super::operon_node_dir().join("bin").join("npm");
    if operon_npm.exists() {
        return Some(operon_npm.to_string_lossy().to_string());
    }
    for path in &["/opt/homebrew/bin/npm", "/usr/local/bin/npm"] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    shell_exec("which npm")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ─── Menu ────────────────────────────────────────────────────────

pub fn build_menu(
    app: &tauri::App,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    let app_submenu = SubmenuBuilder::new(app, "Operon")
        .item(&PredefinedMenuItem::about(app, Some("About Operon"), None)?)
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    let file_submenu = SubmenuBuilder::new(app, "File")
        .item(&PredefinedMenuItem::close_window(app, None)?)
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

    let window_submenu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .build()?;

    let help_item = MenuItemBuilder::with_id("open-help", "Operon Help").build(app)?;

    let help_submenu = SubmenuBuilder::new(app, "Help").item(&help_item).build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_submenu)
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&window_submenu)
        .item(&help_submenu)
        .build()?;

    Ok(menu)
}

// ─── Dictation ───────────────────────────────────────────────────

pub fn start_dictation(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    let script_path = super::temp_dir().join("operon_dictation.swift");
    let swift_code = r#"
import Foundation
import Speech
import AVFoundation

SFSpeechRecognizer.requestAuthorization { status in
    guard status == .authorized else {
        FileHandle.standardError.write("NOT_AUTHORIZED\n".data(using: .utf8)!)
        exit(1)
    }

    let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "en-US"))!
    let audioEngine = AVAudioEngine()
    let request = SFSpeechAudioBufferRecognitionRequest()
    request.shouldReportPartialResults = true

    let inputNode = audioEngine.inputNode
    let recordingFormat = inputNode.outputFormat(forBus: 0)

    inputNode.installTap(onBus: 0, bufferSize: 1024, format: recordingFormat) { buffer, _ in
        request.append(buffer)
    }

    audioEngine.prepare()
    do {
        try audioEngine.start()
    } catch {
        FileHandle.standardError.write("AUDIO_ERROR\n".data(using: .utf8)!)
        exit(2)
    }

    recognizer.recognitionTask(with: request) { result, error in
        if let result = result {
            let text = result.bestTranscription.formattedString
            let isFinal = result.isFinal
            let prefix = isFinal ? "FINAL" : "PARTIAL"
            print("\(prefix):\(text)")
            fflush(stdout)

            if isFinal {
                audioEngine.stop()
                inputNode.removeTap(onBus: 0)
                exit(0)
            }
        }
        if let error = error {
            let nsError = error as NSError
            if nsError.domain == "kAFAssistantErrorDomain" && nsError.code == 1110 {
                audioEngine.stop()
                inputNode.removeTap(onBus: 0)
                print("DONE:")
                fflush(stdout)
                exit(0)
            }
        }
    }

    DispatchQueue.global().async {
        while let line = readLine() {
            if line == "STOP" {
                audioEngine.stop()
                inputNode.removeTap(onBus: 0)
                request.endAudio()
                Thread.sleep(forTimeInterval: 0.5)
                print("DONE:")
                fflush(stdout)
                exit(0)
            }
        }
    }
}

RunLoop.main.run()
"#;

    std::fs::write(&script_path, swift_code)
        .map_err(|e| format!("Failed to write dictation script: {}", e))?;

    let mut child = std::process::Command::new("swift")
        .arg(&script_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start dictation: {}", e))?;

    let pid = child.id();
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    if let Some(stdin) = child.stdin.take() {
        let mut guard = super::super::commands::settings::DICTATION_PROCESS
            .lock()
            .map_err(|e| e.to_string())?;
        *guard = Some(super::super::commands::settings::DictationProcess { stdin, pid });
    }

    let app1 = app.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(text) = line.strip_prefix("PARTIAL:") {
                let _ = app1.emit(
                    "dictation-result",
                    serde_json::json!({
                        "text": text, "isFinal": false
                    }),
                );
            } else if let Some(text) = line.strip_prefix("FINAL:") {
                let _ = app1.emit(
                    "dictation-result",
                    serde_json::json!({
                        "text": text, "isFinal": true
                    }),
                );
            } else if line.starts_with("DONE:") {
                let _ = app1.emit("dictation-done", "complete");
            }
        }
        let _ = app1.emit("dictation-done", "ended");
        if let Ok(mut guard) = super::super::commands::settings::DICTATION_PROCESS.lock() {
            *guard = None;
        }
    });

    let app2 = app.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("NOT_AUTHORIZED") {
                let _ = app2.emit("dictation-error", "Speech recognition not authorized. Please allow in System Settings → Privacy & Security → Speech Recognition.");
            } else if line.contains("AUDIO_ERROR") {
                let _ = app2.emit("dictation-error", "Could not access microphone. Please allow in System Settings → Privacy & Security → Microphone.");
            }
        }
    });

    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}
