use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use super::mcp::MCPServerConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub font_size: u32,
    pub font_family: String,
    pub tab_size: u32,
    pub word_wrap: bool,
    pub minimap_enabled: bool,
    pub model: String,
    pub max_turns: u32,
    pub max_budget_usd: f64,
    pub show_hidden_files: bool,
    pub terminal_font_size: u32,
    #[serde(default)]
    pub setup_completed: bool,
    #[serde(default)]
    pub mcp_servers: Vec<MCPServerConfig>,
    #[serde(default)]
    pub extension_settings: HashMap<String, serde_json::Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 13,
            font_family: "JetBrains Mono".to_string(),
            tab_size: 2,
            word_wrap: false,
            minimap_enabled: true,
            model: "claude-sonnet-4-20250514".to_string(),
            max_turns: 25,
            max_budget_usd: 5.0,
            show_hidden_files: false,
            terminal_font_size: 13,
            setup_completed: false,
            mcp_servers: Vec::new(),
            extension_settings: HashMap::new(),
        }
    }
}

pub struct SettingsManager {
    pub settings: Mutex<AppSettings>,
}

impl SettingsManager {
    pub fn new() -> Self {
        // Try to load from disk, fall back to defaults
        let settings = Self::load_from_disk().unwrap_or_default();
        Self {
            settings: Mutex::new(settings),
        }
    }

    pub(crate) fn config_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|p| p.join("operon").join("settings.json"))
    }

    fn load_from_disk() -> Option<AppSettings> {
        let path = Self::config_path()?;
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save_to_disk(settings: &AppSettings) -> Result<(), String> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
            std::fs::write(path, data).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn get_settings(
    state: tauri::State<'_, SettingsManager>,
) -> Result<AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
pub async fn update_settings(
    state: tauri::State<'_, SettingsManager>,
    settings: AppSettings,
) -> Result<(), String> {
    SettingsManager::save_to_disk(&settings)?;
    let mut current = state.settings.lock().map_err(|e| e.to_string())?;
    *current = settings;
    Ok(())
}

/// Start macOS native speech recognition using SFSpeechRecognizer + AVAudioEngine.
/// Spawns a background Swift process that streams recognized text back via events.
#[tauri::command]
pub async fn start_dictation(app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    // Write the Swift speech recognition script to a temp file
    let script_path = std::env::temp_dir().join("operon_dictation.swift");
    let swift_code = r#"
import Foundation
import Speech
import AVFoundation

// Request authorization
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
            // Output format: PARTIAL:text or FINAL:text
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
            // Normal end-of-speech errors are ok
            let nsError = error as NSError
            if nsError.domain == "kAFAssistantErrorDomain" && nsError.code == 1110 {
                // No speech detected - normal timeout
                audioEngine.stop()
                inputNode.removeTap(onBus: 0)
                print("DONE:")
                fflush(stdout)
                exit(0)
            }
        }
    }

    // Listen for STOP on stdin
    DispatchQueue.global().async {
        while let line = readLine() {
            if line == "STOP" {
                audioEngine.stop()
                inputNode.removeTap(onBus: 0)
                request.endAudio()
                // Give a moment for final results
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

    // Spawn the Swift process
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

    // Store the child's stdin so we can send STOP later
    if let Some(stdin) = child.stdin.take() {
        // Store in a global so stop_dictation can access it
        let mut guard = DICTATION_PROCESS.lock().map_err(|e| e.to_string())?;
        *guard = Some(DictationProcess { stdin, pid });
    }

    // Read stdout in a background thread and emit events
    let app = app_handle.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                if line.starts_with("PARTIAL:") {
                    let text = &line[8..];
                    let _ = app.emit("dictation-result", serde_json::json!({
                        "text": text,
                        "isFinal": false
                    }));
                } else if line.starts_with("FINAL:") {
                    let text = &line[6..];
                    let _ = app.emit("dictation-result", serde_json::json!({
                        "text": text,
                        "isFinal": true
                    }));
                } else if line.starts_with("DONE:") {
                    let _ = app.emit("dictation-done", "complete");
                }
            }
        }
        // Process ended
        let _ = app.emit("dictation-done", "ended");
        // Clean up global reference
        if let Ok(mut guard) = DICTATION_PROCESS.lock() {
            *guard = None;
        }
    });

    // Read stderr for errors
    let app2 = app_handle.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                if line.contains("NOT_AUTHORIZED") {
                    let _ = app2.emit("dictation-error", "Speech recognition not authorized. Please allow in System Settings → Privacy & Security → Speech Recognition.");
                } else if line.contains("AUDIO_ERROR") {
                    let _ = app2.emit("dictation-error", "Could not access microphone. Please allow in System Settings → Privacy & Security → Microphone.");
                }
            }
        }
    });

    // Wait for process in background
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

struct DictationProcess {
    stdin: std::process::ChildStdin,
    #[allow(dead_code)]
    pid: u32,
}

static DICTATION_PROCESS: std::sync::Mutex<Option<DictationProcess>> = std::sync::Mutex::new(None);

/// Stop the currently running dictation process.
#[tauri::command]
pub async fn stop_dictation() -> Result<(), String> {
    use std::io::Write;
    let mut guard = DICTATION_PROCESS.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut process) = *guard {
        let _ = process.stdin.write_all(b"STOP\n");
        let _ = process.stdin.flush();
    }
    *guard = None;
    Ok(())
}
