use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SessionTab {
    pub file_path: String,
    #[serde(default)]
    pub is_remote: bool,
    #[serde(default)]
    pub remote_profile_id: Option<String>,
    #[serde(default)]
    pub cursor_line: Option<u32>,
    #[serde(default)]
    pub cursor_col: Option<u32>,
    #[serde(default)]
    pub scroll_top: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SessionTerminal {
    pub id: String,
    pub cwd: String,
    #[serde(default)]
    pub is_remote: bool,
    #[serde(default)]
    pub remote_profile_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SessionState {
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub editor_tabs: Vec<SessionTab>,
    #[serde(default)]
    pub active_tab_id: Option<String>,
    #[serde(default)]
    pub terminal_tabs: Vec<SessionTerminal>,
    #[serde(default)]
    pub active_chat_session_id: Option<String>,
    #[serde(default)]
    pub active_sidebar_view: Option<String>,
    #[serde(default)]
    pub saved_at: u64,
}

/// Partial slice sent by callers (Map-shape so we can tell "omitted" from
/// "explicit null"). Each caller (ProjectContext / TerminalArea / etc.)
/// owns disjoint fields; the backend merges so two callers don't clobber
/// each other.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(transparent)]
pub struct SessionStatePatch(pub serde_json::Map<String, serde_json::Value>);

pub struct SessionStateManager {
    inner: Mutex<SessionState>,
}

impl SessionStateManager {
    pub fn new() -> Self {
        let initial = load_from_disk().unwrap_or_default();
        Self {
            inner: Mutex::new(initial),
        }
    }
}

fn session_file_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".operon");
    Some(dir.join("last_session.json"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_from_disk() -> Option<SessionState> {
    let path = session_file_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<SessionState>(&data).ok()
}

fn write_atomic(state: &SessionState) -> Result<(), String> {
    let path = session_file_path().ok_or_else(|| "Could not resolve home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
    }
    let data = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data).map_err(|e| format!("write tmp: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

fn apply_patch(current: &mut SessionState, patch: SessionStatePatch) {
    let map = patch.0;
    if let Some(v) = map.get("project_path") {
        current.project_path = serde_json::from_value(v.clone()).unwrap_or(None);
    }
    if let Some(v) = map.get("editor_tabs") {
        if let Ok(tabs) = serde_json::from_value::<Vec<SessionTab>>(v.clone()) {
            current.editor_tabs = tabs;
        }
    }
    if let Some(v) = map.get("active_tab_id") {
        current.active_tab_id = serde_json::from_value(v.clone()).unwrap_or(None);
    }
    if let Some(v) = map.get("terminal_tabs") {
        if let Ok(ts) = serde_json::from_value::<Vec<SessionTerminal>>(v.clone()) {
            current.terminal_tabs = ts;
        }
    }
    if let Some(v) = map.get("active_chat_session_id") {
        current.active_chat_session_id = serde_json::from_value(v.clone()).unwrap_or(None);
    }
    if let Some(v) = map.get("active_sidebar_view") {
        current.active_sidebar_view = serde_json::from_value(v.clone()).unwrap_or(None);
    }
    current.saved_at = now_secs();
}

#[tauri::command]
pub async fn save_session_state(
    state: tauri::State<'_, SessionStateManager>,
    patch: SessionStatePatch,
) -> Result<(), String> {
    let snapshot = {
        let mut guard = state.inner.lock().map_err(|e| e.to_string())?;
        apply_patch(&mut guard, patch);
        guard.clone()
    };
    write_atomic(&snapshot)
}

#[tauri::command]
pub async fn load_session_state(
    state: tauri::State<'_, SessionStateManager>,
) -> Result<Option<SessionState>, String> {
    let guard = state.inner.lock().map_err(|e| e.to_string())?;
    if guard.saved_at == 0 && guard.editor_tabs.is_empty() && guard.terminal_tabs.is_empty() {
        return Ok(None);
    }
    Ok(Some(guard.clone()))
}

#[tauri::command]
pub async fn clear_session_state(
    state: tauri::State<'_, SessionStateManager>,
) -> Result<(), String> {
    {
        let mut guard = state.inner.lock().map_err(|e| e.to_string())?;
        *guard = SessionState::default();
    }
    if let Some(path) = session_file_path() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
