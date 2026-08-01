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

// A `shell_escape_inner` that wrapped values in double quotes (escaping only `"`)
// used to live here. Double quotes do not stop command substitution or parameter
// expansion, so it was an injection primitive, not an escaper. It is deliberately
// absent rather than fixed: single-quoting is correct for every operand Operon
// sends to a remote shell, and keeping a double-quote helper around invites the
// next call site to use it. If you need a value to expand, build the command so
// the expansion is written literally in the template — never by escaping input.

/// True only for `http`/`https` URLs safe to hand to an OS URL handler.
///
/// `open_url` receives values that came from outside the app — the OAuth URL is
/// scraped out of `claude login`'s PTY output, and catalog entries ship in the
/// bundle — so the scheme is not automatically trustworthy. `file:`, `ms-msdt:`
/// and similar are launchable by the shell and are not what open_url is for.
///
/// Lives here rather than in `platform/windows.rs` so it is compiled and tested
/// on every host; the Windows-only spawn path around it cannot be built on macOS
/// or Linux at all.
pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        // Any ASCII whitespace is rejected, not just newlines: `rundll32` takes the
        // remainder of its command line verbatim after the first space, so a URL
        // containing one would reach the browser with Rust's argv quoting embedded.
        && !url.chars().any(|c| c.is_ascii_whitespace() || c == '\0')
}

#[cfg(test)]
mod url_guard_tests {
    use super::is_web_url;

    #[test]
    fn accepts_the_oauth_urls_the_login_flow_actually_produces() {
        // The real shape: multiple query params joined by `&` — the exact input
        // that `cmd /C start "" <url>` used to truncate at the first `&`.
        for u in [
            "https://claude.com/cai/oauth/authorize?code=abc&state=xyz&scope=org%3Acreate_api_key",
            "https://claude.ai/oauth/authorize?a=1&b=2",
            "http://localhost:1420/callback?x=1&y=2",
        ] {
            assert!(is_web_url(u), "should accept {u}");
        }
    }

    #[test]
    fn rejects_schemes_that_are_not_the_web() {
        for u in [
            "file:///etc/passwd",
            "ms-msdt:/id PCWDiagnostic",
            "javascript:alert(1)",
            "vbscript:msgbox",
            "\\\\attacker\\share",
            "",
            "   ",
            "ftp://example.com",
        ] {
            assert!(!is_web_url(u), "should reject {u:?}");
        }
    }

    #[test]
    fn the_windows_opener_does_not_go_through_cmd_start() {
        // `open_url` on Windows cannot be compiled or executed here, so assert on
        // its source. The original bug was `cmd /C start "" <url>`: cmd re-parses
        // its command line and `&` is a separator, truncating every OAuth URL at
        // its first query parameter. Needles are assembled at runtime so they
        // cannot match this assertion's own text.
        let src = include_str!("windows.rs");
        assert!(
            src.contains(&format!("url.dll,{}", "FileProtocolHandler")),
            "windows open_url should hand the URL to rundll32 as a single argv entry"
        );
        let bad = format!("\"{}\", \"{}\", \"\"", "/C", "start");
        assert!(
            !src.contains(&bad),
            "windows open_url is passing the URL through `cmd /C start` argv again"
        );
        assert!(
            src.contains(&format!("is_{}web_url", "")),
            "windows open_url should reject non-http(s) schemes"
        );
    }

    #[test]
    fn rejects_whitespace_that_would_split_a_command_line() {
        assert!(!is_web_url("https://ok.example/a b"));
        assert!(!is_web_url("https://ok.example/a\tb"));
    }

    #[test]
    fn rejects_embedded_line_breaks() {
        // A newline would let a second command ride along in shells that split on it.
        assert!(!is_web_url("https://ok.example/\r\nstart calc"));
        assert!(!is_web_url("https://ok.example/\nfoo"));
    }
}
