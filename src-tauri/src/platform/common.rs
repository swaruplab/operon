//! Shared utilities used across all platforms.

/// Normalize a path for display — convert backslashes to forward slashes.
pub fn normalize_display_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Shell-escape a string by wrapping it in single quotes for bash/zsh.
///
/// Operon runs every command string through a POSIX shell on all platforms
/// (Git Bash on Windows — cmd.exe cannot parse the codebase's command
/// strings), so POSIX single-quote escaping is correct everywhere. Single
/// quotes also preserve Windows backslash paths verbatim.
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Shell-escape for embedding inside double quotes.
pub fn shell_escape_inner(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}
