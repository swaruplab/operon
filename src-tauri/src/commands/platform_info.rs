//! Platform capability reporting.
//!
//! Exposes the host OS and a set of authoritative feature flags to the
//! frontend, replacing fragile user-agent sniffing and scattered
//! `isWindows` / `isLinux` checks. The frontend loads this once at startup
//! (see `src/lib/platform.ts`) and gates platform-specific UI off the flags.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// Host operating system: "macos" | "windows" | "linux".
    pub os: String,
    /// Path to Git Bash on Windows (used by Claude Code); None elsewhere.
    pub git_bash_path: Option<String>,
    /// Whether SSH ControlMaster multiplexing is supported.
    pub supports_ssh_mux: bool,
    /// Whether the bundled Anthropic→OpenAI translation proxy sidecar exists.
    /// Only macOS ships the `anthropic-proxy-rs` binary (Windows is a stub,
    /// Linux has no binary), so this is true exclusively on macOS.
    pub translation_proxy_supported: bool,
}

/// Report the host platform and its capability flags.
#[tauri::command]
pub fn get_platform_info() -> Result<PlatformInfo, String> {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
    .to_string();

    Ok(PlatformInfo {
        os,
        git_bash_path: crate::platform::find_git_bash_path(),
        supports_ssh_mux: crate::platform::supports_ssh_mux(),
        translation_proxy_supported: cfg!(target_os = "macos"),
    })
}
