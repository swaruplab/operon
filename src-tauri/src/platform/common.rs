//! Shared utilities used across all platforms.

/// Normalize a path for display — convert backslashes to forward slashes.
pub fn normalize_display_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Shell-escape a string by wrapping in single quotes and escaping embedded quotes.
/// Works for bash/zsh on macOS/Linux. Windows uses different escaping.
pub fn shell_escape(s: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        // On Windows, use double quotes and escape embedded double quotes
        format!("\"{}\"", s.replace('"', "\\\""))
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Shell-escape for embedding inside double quotes.
pub fn shell_escape_inner(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}
