use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as AsyncCommand;

/// Suppress console window creation on Windows for std::process::Command.
#[cfg(windows)]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000)
}
#[cfg(not(windows))]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

/// Suppress console window creation on Windows for tokio::process::Command.
#[cfg(windows)]
fn hide_window_async(cmd: &mut AsyncCommand) -> &mut AsyncCommand {
    cmd.creation_flags(0x08000000)
}
#[cfg(not(windows))]
fn hide_window_async(cmd: &mut AsyncCommand) -> &mut AsyncCommand {
    cmd
}

/// Detect whether a Portkey model slug refers to an Anthropic-family model.
/// Slugs look like `@workspace/model-id` (e.g.
/// `@zotgpt-api-bedrock/us.anthropic.claude-opus-4-8`). Anthropic models have
/// `claude` or `anthropic` in the model-id; everything else (Moonshot Kimi,
/// GPT, Gemini, …) must be routed via the OpenAI-format translation proxy.
///
/// Empty / unknown slugs default to `true` so the safe (current) direct path
/// is used until the user explicitly picks a non-Anthropic model.
fn is_anthropic_portkey_model(slug: &str) -> bool {
    let trimmed = slug.trim();
    if trimmed.is_empty() {
        return true;
    }
    let model_id = trimmed.rsplit('/').next().unwrap_or(trimmed).to_lowercase();
    model_id.contains("claude") || model_id.contains("anthropic")
}

/// Build the env vars to pass to Claude Code based on the AI provider setting.
///
/// - "anthropic" (default): sets `ANTHROPIC_API_KEY` from the in-memory key (if any).
/// - "custom": sets `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN`, which Claude
///   Code respects for OpenAI-compatible proxies. When `use_translation_proxy`
///   is enabled AND the bundled `anthropic-proxy` sidecar is running, the base
///   URL is rewritten to the local proxy so backends that only speak OpenAI
///   Chat Completions (Ollama, vLLM, LM Studio pre-0.4.1) work transparently.
/// - "portkey": sets `ANTHROPIC_BASE_URL` to the configured Portkey gateway
///   (institutional or self-hosted) and `ANTHROPIC_AUTH_TOKEN` to the user's
///   virtual API key. Portkey's Anthropic passthrough means Claude Code talks
///   `/v1/messages` directly — no translation proxy, works on Windows + HPC.
fn ai_provider_env(
    settings_state: &tauri::State<'_, super::settings::SettingsManager>,
    proxy_state: &tauri::State<'_, super::proxy::ProxyManager>,
    fallback_key: &Option<String>,
) -> Vec<(String, String)> {
    let settings = match settings_state.settings.lock() {
        Ok(s) => s.clone(),
        Err(_) => return Vec::new(),
    };
    if settings.ai_provider == "portkey" {
        let base = settings.portkey_base_url.trim();
        let key = settings.portkey_api_key.trim();
        if !base.is_empty() && !key.is_empty() {
            let model = settings.portkey_model.trim();

            // Non-Anthropic models (Moonshot Kimi, GPT, Gemini, …) only work
            // via Portkey's OpenAI Chat-Completions surface, not Anthropic's
            // /v1/messages. Route them through the bundled `anthropic-proxy`
            // sidecar, which translates Claude-Code's Anthropic requests into
            // OpenAI format. The frontend starts the proxy with
            // UPSTREAM_BASE_URL=<portkey base> and UPSTREAM_API_KEY=<virtual
            // key> when the user picks a non-Anthropic model.
            if !is_anthropic_portkey_model(model) {
                if let Some(proxy_url) = proxy_state.proxy_base_url() {
                    return vec![
                        ("ANTHROPIC_BASE_URL".to_string(), proxy_url),
                        // Proxy auths upstream itself via UPSTREAM_API_KEY;
                        // this token is only here so Claude Code thinks it
                        // has credentials and proceeds.
                        ("ANTHROPIC_AUTH_TOKEN".to_string(), "portkey".to_string()),
                        // These models can't hit the literal "invalid beta flag"
                        // 400 (the proxy translates to OpenAI and drops the
                        // anthropic-beta header), but the same suppression keeps
                        // Claude Code from emitting beta-only tool-schema fields
                        // (defer_loading, eager_input_streaming), extended-thinking
                        // blocks, and cache_control that the translator would have
                        // to handle — clean, standard requests for the proxy.
                        ("DISABLE_PROMPT_CACHING".to_string(), "1".to_string()),
                        (
                            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
                            "1".to_string(),
                        ),
                        (
                            "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS".to_string(),
                            "1".to_string(),
                        ),
                        ("MAX_THINKING_TOKENS".to_string(), "0".to_string()),
                        ("ANTHROPIC_BETAS".to_string(), String::new()),
                    ];
                }
                // Proxy isn't running yet — fall through to the direct path,
                // which Portkey will reject for non-Anthropic models with a
                // clear error. The frontend should have started the proxy
                // already; if it didn't, the error tells the user to re-open
                // settings.
            }

            // Anthropic-family models: direct passthrough to Portkey's
            // Anthropic-compatible /v1/messages endpoint.
            //
            // Claude Code's Anthropic SDK appends "/v1/messages" to
            // ANTHROPIC_BASE_URL. If the user-stored URL already has "/v1"
            // (the convention for OpenAI-style SDK config — what we ask users
            // to enter), strip it to avoid the SDK constructing
            // "/v1/v1/messages", which Portkey accepts with a 200 + malformed
            // body and causes Claude Code to emit "empty or malformed response".
            let base_for_sdk = base
                .trim_end_matches('/')
                .trim_end_matches("/v1")
                .trim_end_matches('/')
                .to_string();
            // Suppress the `anthropic-beta` headers that Bedrock-backed Portkey
            // routes reject ("400 ... invalid beta flag"). Claude Code 2.1.69+
            // sends `advanced-tool-use-2025-11-20` by DEFAULT, which Bedrock
            // doesn't support. `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS=1` is the
            // documented lever for that (and strips the beta-specific tool-schema
            // fields too). NOTE: `ANTHROPIC_BETAS` *enables* betas — an empty
            // value is NOT a reliable suppressor — but we leave it empty so no
            // user-added betas sneak in. We do NOT set CLAUDE_CODE_USE_BEDROCK=1
            // because that flips auth to AWS SigV4 and overrides the Bearer token
            // Portkey needs via ANTHROPIC_AUTH_TOKEN.
            // Upstream caveat: on some 2.1.69+ builds the disable flag still
            // leaks `advanced-tool-use` (anthropics/claude-code#22893) — if so the
            // header must be stripped gateway-side or Claude Code pinned < 2.1.69.
            let env = vec![
                ("ANTHROPIC_BASE_URL".to_string(), base_for_sdk),
                ("ANTHROPIC_AUTH_TOKEN".to_string(), key.to_string()),
                ("DISABLE_PROMPT_CACHING".to_string(), "1".to_string()),
                (
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
                    "1".to_string(),
                ),
                (
                    "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS".to_string(),
                    "1".to_string(),
                ),
                ("MAX_THINKING_TOKENS".to_string(), "0".to_string()),
                ("ANTHROPIC_BETAS".to_string(), String::new()),
            ];
            return env;
        }
        // Misconfigured Portkey — fall through to direct Anthropic key so the
        // session can at least start with an obvious error rather than a
        // silent network failure.
    }
    let upstream = settings.custom_base_url.trim();
    if settings.ai_provider == "custom" && !upstream.is_empty() {
        // If the translation proxy is running and the user wants it, Claude
        // Code should hit the local proxy instead of the upstream directly.
        let effective_base = if settings.use_translation_proxy {
            // Through the local proxy, which listens on Anthropic /v1/messages —
            // the SDK's appended path lands correctly, so no /v1 stripping.
            proxy_state
                .proxy_base_url()
                .unwrap_or_else(|| upstream.to_string())
        } else {
            // Direct Anthropic path (OpenRouter, Anthropic-compatible LiteLLM).
            // Claude Code's SDK appends "/v1/messages" to ANTHROPIC_BASE_URL, so
            // a base ending in "/v1" (our stored convention — the OpenAI /models
            // + /chat/completions detection probes need it) would become
            // "/v1/v1/messages" → 404 (verified against OpenRouter). Strip a
            // trailing "/v1" here, exactly like the Portkey branch above.
            upstream
                .trim_end_matches('/')
                .trim_end_matches("/v1")
                .trim_end_matches('/')
                .to_string()
        };
        let token = if !settings.custom_api_key.is_empty() {
            settings.custom_api_key.clone()
        } else {
            // Many local endpoints (Ollama, LM Studio) accept any non-empty token.
            // The bundled proxy also tolerates this — it uses UPSTREAM_API_KEY
            // (set at spawn time) to auth upstream, and doesn't re-check the
            // inbound Authorization header.
            "local".to_string()
        };
        vec![
            ("ANTHROPIC_BASE_URL".to_string(), effective_base),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), token),
        ]
    } else if let Some(k) = fallback_key {
        vec![("ANTHROPIC_API_KEY".to_string(), k.clone())]
    } else {
        Vec::new()
    }
}

/// Every credential/routing variable Operon takes ownership of. Claude Code
/// picks its auth source from the environment, so any of these left over from
/// the user's shell profile silently outranks what Operon intends.
pub(crate) const MANAGED_AUTH_VARS: [&str; 3] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
];

/// Env vars that must be cleared (unset / `cmd.env_remove`) so a value the
/// user's shell profile (~/.bashrc / ~/.zshrc) or the inherited environment may
/// have set can't decide which credential Claude Code uses.
///
/// This is the exact complement of [`ai_provider_env`]: every variable in
/// [`MANAGED_AUTH_VARS`] that we did NOT just set is cleared. Deriving it from
/// the env we emit (rather than from the provider name) makes it impossible for
/// the two to disagree — in particular it can never `env_remove` a variable we
/// just `env`'d, which the previous provider-name-keyed version could.
///
/// What this buys per provider:
/// - "anthropic" + user-supplied API key — clears ANTHROPIC_BASE_URL and
///   ANTHROPIC_AUTH_TOKEN. A leftover `export ANTHROPIC_BASE_URL=...` (from
///   prior Portkey / custom-endpoint tinkering) would route Claude requests to
///   the wrong gateway and produce a `400 Either x-portkey-config or
///   x-portkey-provider header is required` from Portkey, or a `404` from any
///   other proxy that doesn't speak Anthropic.
/// - "anthropic" + NO key (Claude Max / Pro subscription — the common case) —
///   also clears ANTHROPIC_API_KEY. Operon supplies no credential here on
///   purpose: the Claude Code CLI owns the login. A stale
///   `export ANTHROPIC_API_KEY=...` in the profile takes precedence over the
///   claude.ai session, and the CLI then reports
///   "Invalid API key · claude.ai connectors are disabled because
///   ANTHROPIC_API_KEY or another auth source is set and takes precedence over
///   your claude.ai login" — a valid subscription that simply never gets used.
/// - "custom" / "portkey" — clears ANTHROPIC_API_KEY. With both API_KEY and
///   AUTH_TOKEN set, the Anthropic SDK sends `x-api-key`, which most bearer-only
///   proxies (Ollama, vLLM, anthropic-proxy, Portkey virtual-key routes) reject
///   or treat as anonymous.
fn ai_provider_env_unset(env: &[(String, String)]) -> Vec<&'static str> {
    MANAGED_AUTH_VARS
        .into_iter()
        .filter(|var| !env.iter().any(|(k, _)| k == var))
        .collect()
}

/// Render the env vars as a `unset X Y; export K='V'; ...` string for injection
/// into remote shell scripts. The `unset` prefix runs AFTER the user's shell
/// profile has been sourced — so it clears any stale provider variables the
/// profile may have re-exported before our own `export`s take effect.
fn ai_provider_env_exports(env: &[(String, String)], unset: &[&str]) -> String {
    let mut out = String::new();
    if !unset.is_empty() {
        out.push_str(&format!("unset {}; ", unset.join(" ")));
    }
    for (k, v) in env {
        out.push_str(&format!("export {}='{}'; ", k, v.replace('\'', "'\\''")));
    }
    out
}

/// Prefix name for the staging variables described in [`local_env_prefix`].
const LOCAL_ENV_STAGE_PREFIX: &str = "OPERON_ENV_";

/// The local equivalent of [`ai_provider_env_exports`].
///
/// `bash -l -c '<cmd>'` sources the user's profile BEFORE running `<cmd>`, which
/// means anything the profile exports **overwrites what we passed via
/// `cmd.env`** — an `export ANTHROPIC_API_KEY=...` in `~/.zshrc` beats the key
/// the user typed into Operon, and a stale `ANTHROPIC_BASE_URL` beats the
/// gateway Operon selected. Only statements inside `<cmd>` run late enough to
/// win. Remote sessions already re-assert their exports this way; local ones
/// did not, so they silently ran on whatever the profile said.
///
/// Values are staged through `OPERON_ENV_<NAME>` (set via `cmd.env`, which the
/// profile has no reason to touch) and re-exported by name here, so a credential
/// never appears in the command line where `ps` would show it. The staging vars
/// are unset immediately after so they don't leak into the agent's own shell.
fn local_env_prefix(env: &[(String, String)], unset: &[&str]) -> String {
    let mut out = String::new();
    if !unset.is_empty() {
        out.push_str(&format!("unset {}; ", unset.join(" ")));
    }
    for (k, _) in env {
        out.push_str(&format!(
            "export {k}=\"${{{p}{k}}}\"; unset {p}{k}; ",
            k = k,
            p = LOCAL_ENV_STAGE_PREFIX
        ));
    }
    out
}

/// The shell used to launch Claude sessions with `-l -c` flags.
/// On Windows, Git Bash is required because cmd.exe doesn't support `-l`/`-c`
/// and Claude Code itself needs a POSIX environment. Returns a clear error if
/// Git Bash is missing rather than silently falling back to cmd.exe (which
/// would fail opaquely once `-l -c` is appended).
fn claude_shell() -> Result<String, String> {
    crate::platform::posix_shell()
}

/// Guarantee a sane baseline PATH for any local bash login-shell we spawn.
///
/// When Operon is launched from Finder/Spotlight, launchd hands it a very
/// minimal env (no /usr/bin on some setups). Spawning `bash -l -c "..."` then
/// causes bash to source the user's `.bash_profile` BEFORE running the
/// command — and if that profile uses `id`, `uname`, `stat`, or any other
/// coreutil that isn't on the inherited PATH, the profile emits "command not
/// found" to stderr. Operon's stderr-reader treats that as a fatal session
/// error even though the actual command runs fine.
///
/// Fix: prepend the standard macOS / Linux binary directories to PATH before
/// bash forks. The user's existing PATH (if any) wins for everything beyond
/// the baseline, so this doesn't shadow custom toolchains.
#[cfg(not(target_os = "windows"))]
fn enforce_baseline_path(cmd: &mut tokio::process::Command) {
    // System dirs FIRST so coreutils a login profile calls (id/uname/stat)
    // resolve even under launchd's minimal env (the original purpose here).
    const SYSTEM: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
    // Then every known tool-install dir (homebrew, /usr/local/bin, ~/.local/bin,
    // ~/.claude/local/bin, ~/.npm-global/bin, Operon-managed node). This makes a
    // bare `claude` (or `node`, for an npm-shim install) resolve even when Operon
    // was launched from Finder/Dock with launchd's minimal PATH and the user's
    // PATH lines live only in .zshrc — the exact "command not found: claude"
    // case. Belt-and-suspenders with the absolute-path substitution below.
    let mut parts = vec![SYSTEM.to_string()];
    parts.extend(
        crate::platform::extra_tool_paths()
            .iter()
            .map(|p| p.to_string_lossy().to_string()),
    );
    let inherited = std::env::var("PATH").unwrap_or_default();
    if !inherited.is_empty() {
        parts.push(inherited);
    }
    cmd.env("PATH", parts.join(":"));
}

#[cfg(target_os = "windows")]
fn enforce_baseline_path(_cmd: &mut tokio::process::Command) {
    // Windows: PATH baseline is handled separately by claude_shell()'s
    // Git Bash resolver and CLAUDE_CODE_GIT_BASH_PATH.
}

/// Replace the `claude` command word in a built command string with an absolute,
/// quoted path (when resolvable) so the spawned login shell never depends on its
/// PATH to find `claude`. Anchored to shell-token boundaries — the start of the
/// string, or a `| ` pipe — so a single-quoted file path that merely CONTAINS
/// "claude " (e.g. a project under `~/claude tools/`) is never rewritten. No-op
/// when the path can't be resolved (keeps bare `claude`) and on Windows (where
/// `claude_invocation()` intentionally returns the bare name).
fn substitute_claude_invocation(cmd: &str) -> String {
    replace_claude_token(cmd, &crate::platform::claude_invocation())
}

/// Pure anchoring logic for [`substitute_claude_invocation`] (separated so it can
/// be unit-tested without touching the filesystem). Rewrites the `claude` command
/// word only at a shell-token boundary — the very start of the string, or right
/// after a `| ` pipe — never an arbitrary substring.
fn replace_claude_token(cmd: &str, invoke: &str) -> String {
    if invoke == "claude" {
        return cmd.to_string();
    }
    if let Some(rest) = cmd.strip_prefix("claude ") {
        format!("{} {}", invoke, rest)
    } else if let Some(idx) = cmd.find("| claude ") {
        format!(
            "{}| {} {}",
            &cmd[..idx],
            invoke,
            &cmd[idx + "| claude ".len()..]
        )
    } else {
        cmd.to_string()
    }
}

#[cfg(test)]
mod oauth_detection_tests {
    use super::{json_has_oauth_material, oauth_credential_files};

    fn v(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("valid test JSON")
    }

    #[test]
    fn the_mcp_cache_that_faked_a_login_is_rejected() {
        // Verbatim shape of ~/.claude/mcp-needs-auth-cache.json. Its NAME contains
        // "auth" and its body is non-empty, which is all the old check required —
        // so Operon reported an authenticated Claude subscription on the strength
        // of a list of MCP servers. It carries no token and must not qualify.
        let cache = v(r#"{
            "claude.ai BioRender": {"timestamp": 1785083247017, "id": "mcpsrv_01Mk"},
            "claude.ai Synapse.org": {"timestamp": 1785083247047, "id": "mcpsrv_01NU"}
        }"#);
        assert!(!json_has_oauth_material(&cache));
    }

    #[test]
    fn a_real_credential_file_is_accepted_nested_or_flat() {
        // Current CLI shape: nested under claudeAiOauth.
        let nested = v(
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-abc","refreshToken":"sk-ant-ort01-def","expiresAt":123}}"#,
        );
        assert!(json_has_oauth_material(&nested));

        // Older flat shape, and the snake_case spelling.
        assert!(json_has_oauth_material(&v(
            r#"{"access_token":"sk-ant-oat01-abc"}"#
        )));
        assert!(json_has_oauth_material(&v(
            r#"{"refreshToken":"sk-ant-ort01-def"}"#
        )));
    }

    #[test]
    fn empty_or_placeholder_tokens_do_not_count() {
        assert!(!json_has_oauth_material(&v(r#"{}"#)));
        assert!(!json_has_oauth_material(&v(r#"null"#)));
        assert!(!json_has_oauth_material(&v(
            r#"{"claudeAiOauth":{"accessToken":""}}"#
        )));
        assert!(!json_has_oauth_material(&v(
            r#"{"claudeAiOauth":{"accessToken":"   "}}"#
        )));
        // A key that merely mentions auth, with no token in it.
        assert!(!json_has_oauth_material(&v(
            r#"{"needsAuth":true,"authCache":{"server":"x"}}"#
        )));
    }

    #[test]
    fn only_real_credential_paths_are_consulted() {
        // Exact paths, never a filename substring — that is what let the MCP
        // cache through.
        for p in oauth_credential_files() {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                name == ".credentials.json" || name == "credentials.json",
                "unexpected credential path considered: {}",
                p.display()
            );
        }
    }
}

#[cfg(test)]
mod auth_env_tests {
    use super::{ai_provider_env_exports, ai_provider_env_unset, local_env_prefix};

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn subscription_session_clears_every_credential() {
        // ai_provider_env returns nothing for "anthropic" with no in-app key —
        // the CLI's own claude.ai login is the credential. Anything left in the
        // environment outranks it, so all three must go. This is the regression
        // behind "connectors are disabled because ANTHROPIC_API_KEY ... takes
        // precedence over your claude.ai login".
        let unset = ai_provider_env_unset(&env(&[]));
        assert!(unset.contains(&"ANTHROPIC_API_KEY"));
        assert!(unset.contains(&"ANTHROPIC_AUTH_TOKEN"));
        assert!(unset.contains(&"ANTHROPIC_BASE_URL"));
    }

    #[test]
    fn never_clears_a_variable_we_just_set() {
        // The whole point of deriving the unset list from the emitted env: an
        // overlap would make `cmd.env_remove` delete the credential `cmd.env`
        // just supplied, and the shell prefix `unset` it before claude runs.
        for supplied in [
            env(&[("ANTHROPIC_API_KEY", "sk-ant-test")]),
            env(&[
                ("ANTHROPIC_BASE_URL", "https://api.portkey.ai"),
                ("ANTHROPIC_AUTH_TOKEN", "virtual-key"),
            ]),
        ] {
            let unset = ai_provider_env_unset(&supplied);
            for (k, _) in &supplied {
                assert!(
                    !unset.contains(&k.as_str()),
                    "{k} was both set and unset — the two lists must be disjoint"
                );
            }
        }
    }

    #[test]
    fn api_key_session_still_clears_gateway_vars() {
        let unset = ai_provider_env_unset(&env(&[("ANTHROPIC_API_KEY", "sk-ant-test")]));
        assert_eq!(unset, vec!["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL"]);
    }

    #[test]
    fn gateway_session_still_clears_the_api_key() {
        // Both API_KEY and AUTH_TOKEN set makes the SDK send x-api-key, which
        // bearer-only proxies reject.
        let supplied = env(&[
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:11434"),
            ("ANTHROPIC_AUTH_TOKEN", "local"),
        ]);
        assert_eq!(ai_provider_env_unset(&supplied), vec!["ANTHROPIC_API_KEY"]);
    }

    #[test]
    fn local_prefix_reexports_by_reference_never_by_value() {
        // The credential must not appear in the command line — it goes in via
        // cmd.env under OPERON_ENV_*, and the prefix only names that variable.
        let supplied = env(&[("ANTHROPIC_API_KEY", "sk-ant-secret")]);
        let prefix = local_env_prefix(&supplied, &ai_provider_env_unset(&supplied));
        assert!(
            !prefix.contains("sk-ant-secret"),
            "secret leaked into argv: {prefix}"
        );
        assert!(prefix.contains("export ANTHROPIC_API_KEY=\"${OPERON_ENV_ANTHROPIC_API_KEY}\";"));
        // The staging var is cleaned up so it doesn't leak into the agent's shell.
        assert!(prefix.contains("unset OPERON_ENV_ANTHROPIC_API_KEY;"));
        // ...and the vars we are NOT supplying are still cleared.
        assert!(prefix.starts_with("unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL; "));
    }

    #[test]
    fn local_prefix_is_only_an_unset_for_subscription_sessions() {
        // No credential supplied -> nothing to re-export, everything cleared.
        let prefix = local_env_prefix(&[], &ai_provider_env_unset(&[]));
        assert_eq!(
            prefix,
            "unset ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL; "
        );
    }

    #[test]
    fn remote_prefix_unsets_before_it_exports() {
        // Order matters on the remote: the prefix is spliced in after the user's
        // profile has been sourced, so `unset` has to run before our exports.
        let supplied = env(&[("ANTHROPIC_API_KEY", "sk-ant-test")]);
        let rendered = ai_provider_env_exports(&supplied, &ai_provider_env_unset(&supplied));
        let unset_at = rendered.find("unset ").expect("unset prefix present");
        let export_at = rendered.find("export ").expect("export present");
        assert!(unset_at < export_at, "got: {rendered}");
        assert!(rendered.contains("export ANTHROPIC_API_KEY='sk-ant-test';"));
    }
}

#[cfg(test)]
mod claude_token_tests {
    use super::replace_claude_token;

    #[test]
    fn rewrites_leading_command() {
        assert_eq!(
            replace_claude_token("claude -p 'hi' --verbose", "'/Users/x/.local/bin/claude'"),
            "'/Users/x/.local/bin/claude' -p 'hi' --verbose"
        );
    }

    #[test]
    fn rewrites_after_pipe_in_report_mode() {
        assert_eq!(
            replace_claude_token("cat '/tmp/p.txt' | claude -p --verbose", "'/abs/claude'"),
            "cat '/tmp/p.txt' | '/abs/claude' -p --verbose"
        );
    }

    #[test]
    fn never_touches_claude_inside_a_quoted_path() {
        // A project dir that merely contains "claude " must NOT be rewritten.
        let cmd = "cat '/Users/x/claude tools/p.txt' | claude -p";
        assert_eq!(
            replace_claude_token(cmd, "'/abs/claude'"),
            "cat '/Users/x/claude tools/p.txt' | '/abs/claude' -p"
        );
    }

    #[test]
    fn noop_when_unresolved() {
        let cmd = "claude -p 'hi'";
        assert_eq!(replace_claude_token(cmd, "claude"), cmd);
    }
}

/// True if a line of stderr looks like a benign coreutil-not-found from the
/// user's shell profile (e.g. `bash: line 12: id: command not found`).
/// We only suppress these for a small fixed list of coreutils that a profile
/// might legitimately call — never for `claude`, `node`, `npm`, etc., so a
/// real "binary not found" failure is still surfaced.
fn is_profile_coreutil_warning(line: &str) -> bool {
    // Match the bash-error shape: "bash: ... : <util>: ..." where <util> is
    // a known coreutil that profiles commonly invoke before PATH is set up.
    const BENIGN_UTILS: &[&str] = &[
        "id", "uname", "hostname", "tty", "stty", "tput", "stat", "uptime", "whoami", "groups",
        "logname", "basename", "dirname",
    ];
    let l = line.trim();
    // Two common shapes:
    //   bash: line N: <util>: command not found
    //   bash: <file>: line N: <util>: command not found
    // And the "No such file or directory" variant for the binary itself.
    if !l.starts_with("bash:") && !l.starts_with("zsh:") && !l.starts_with("sh:") {
        return false;
    }
    if !l.contains("command not found") && !l.contains("No such file or directory") {
        return false;
    }
    BENIGN_UTILS.iter().any(|u| {
        // Match ": <util>: " so we don't catch "myid: command not found".
        l.contains(&format!(": {}:", u))
    })
}

// --- Types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub method: String, // "api_key", "oauth", "none"
}

/// Persistent metadata about a Claude session, saved to ~/.operon/sessions/
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionMetadata {
    pub session_id: String,                // Our frontend UUID
    pub claude_session_id: Option<String>, // Claude CLI's internal session ID (for --resume)
    pub project_path: String,              // Local or remote working directory
    pub profile_id: Option<String>,        // SSH profile ID if remote
    pub remote_path: Option<String>,       // Remote path if remote
    pub mode: String,                      // "agent", "plan", "ask"
    pub model: Option<String>,
    pub created_at: u64,             // Unix timestamp ms
    pub last_activity: u64,          // Unix timestamp ms
    pub status: String,              // "running", "completed", "failed", "stopped"
    pub use_terminal: bool,          // Whether this used terminal mode
    pub terminal_id: Option<String>, // Terminal ID if terminal mode
    #[serde(default)]
    pub name: Option<String>, // Human-readable session name (from first prompt)
}

/// Status of a session's output files on the filesystem
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionFileStatus {
    pub session_id: String,
    pub output_exists: bool,
    pub done_exists: bool,
    pub is_running: bool,   // output exists but done doesn't
    pub is_completed: bool, // both exist
}

/// PATH prefix applied to every remote SSH command that invokes `claude`.
/// Covers all known install locations so `claude` is found regardless of shell or rc files.
const REMOTE_PATH_PREFIX: &str =
    r#"export PATH="$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH"; "#;

pub struct ClaudeSession {
    pub child: tokio::process::Child,
}

pub struct ClaudeManager {
    pub sessions: Mutex<HashMap<String, ClaudeSession>>,
    pub api_key: Mutex<Option<String>>,
    /// Per-remote-host single-flight guard so two of Operon's OWN `claude`
    /// launches never trigger an OAuth token refresh at the same instant. On a
    /// shared-NFS HPC home a refresh rotates+invalidates the token, and a lost
    /// race can permanently wipe the refresh token (see check_remote_claude_auth).
    /// Keyed by SSH profile id. The inner Arc<tokio::Mutex> is the real lock; this
    /// outer std Mutex only guards the map and is never held across `.await`.
    pub refresh_locks: Mutex<HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
}

impl ClaudeManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            api_key: Mutex::new(None),
            refresh_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Get-or-create the per-host OAuth-refresh serialization lock for a profile.
    /// Holds the map's std Mutex only briefly (no `.await` inside) and returns a
    /// cloned Arc the caller can `.lock_owned().await` — keeping std locks off
    /// the await path (project rule #10).
    pub fn refresh_lock_for(&self, profile_id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
        let mut map = self.refresh_locks.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(profile_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Best-effort kill of every tracked session. Drains the map under the lock,
    /// then issues a synchronous kill outside it (no `.await`, so it's safe to
    /// call from the window close handler). Individual errors are ignored so
    /// shutdown never hangs.
    pub fn kill_all(&self) {
        // Recover from a poisoned lock (into_inner) rather than bailing — on
        // shutdown we must still reap these children, or local Claude/node
        // agents orphan past app exit.
        let children: Vec<tokio::process::Child> = {
            let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
            sessions.drain().map(|(_, s)| s.child).collect()
        };
        for mut child in children {
            let _ = child.start_kill();
        }
    }
}

// --- Session Metadata Persistence ---

fn sessions_dir() -> Result<std::path::PathBuf, String> {
    crate::platform::sessions_dir()
}

fn save_session_to_disk(meta: &SessionMetadata) -> Result<(), String> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", meta.session_id));
    let data = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| format!("Failed to save session: {}", e))
}

fn load_session_from_disk(session_id: &str) -> Result<Option<SessionMetadata>, String> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", session_id));
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let meta: SessionMetadata = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(Some(meta))
}

fn load_all_sessions_from_disk() -> Vec<SessionMetadata> {
    let dir = match sessions_dir() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut sessions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<SessionMetadata>(&data) {
                        sessions.push(meta);
                    }
                }
            }
        }
    }
    // Sort by last_activity descending (most recent first)
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_activity));
    sessions
}

// --- Detection & Installation ---

/// Helper: run a command through the user's login shell to get proper PATH.
/// Delegates to the platform abstraction layer.
fn login_shell_cmd(command: &str) -> std::process::Command {
    crate::platform::shell_exec(command)
}

#[tauri::command]
pub async fn check_claude_installed() -> Result<ClaudeStatus, String> {
    // Use platform-aware tool discovery (where.exe on Windows, which on Unix)
    let tool_info = crate::platform::check_tool("claude");

    if tool_info.is_none() {
        return Ok(ClaudeStatus {
            installed: false,
            version: None,
            path: None,
        });
    }

    // `check_tool` already returns a version — resolved via the same path it
    // found (login shell OR the Finder-safe dir-probe), so don't re-run a bare
    // `claude --version` here (that fails on Finder/Dock launches).
    let (path, version_str) = tool_info.unwrap();
    let version = Some(version_str)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    Ok(ClaudeStatus {
        installed: true,
        version,
        path: Some(path),
    })
}

/// Status of the OpenSSH client (`ssh`), used by the setup wizard.
/// Windows 10/11 ship an OpenSSH client but it may be disabled.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshStatus {
    pub available: bool,
    pub path: Option<String>,
}

/// Check whether the OpenSSH client (`ssh`) is available on this machine.
/// Uses the platform-aware tool discovery (`where.exe ssh` on Windows,
/// `which ssh` on Unix) — same path as `check_claude_installed`.
#[tauri::command]
pub async fn check_ssh_available() -> Result<SshStatus, String> {
    let tool_info = crate::platform::check_tool("ssh");

    Ok(match tool_info {
        Some((path, _)) => SshStatus {
            available: true,
            path: Some(path),
        },
        None => SshStatus {
            available: false,
            path: None,
        },
    })
}

/// Install Claude Code using platform-appropriate methods.
/// Delegates to the platform abstraction layer which handles:
/// - macOS: curl installer → npm fallback → Terminal.app fallback
/// - Windows: npm install
/// - Linux: curl installer → npm fallback
#[tauri::command]
pub async fn install_claude(_method: String) -> Result<(), String> {
    crate::platform::install_claude_platform()
}

// --- Dependency Checking for Setup Wizard ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DependencyStatus {
    pub xcode_cli: bool,
    pub node: bool,
    pub node_version: Option<String>,
    pub npm: bool,
    pub npm_version: Option<String>,
    pub claude_code: bool,
    pub claude_version: Option<String>,
    pub git_bash: bool,
}

/// Check all local dependencies needed for Claude Code
#[tauri::command]
pub async fn check_local_dependencies() -> Result<DependencyStatus, String> {
    // Refresh PATH from the registry FIRST. On Windows, tools just installed by
    // the elevated batch land on the system PATH / in known dirs, but this
    // (launch-time) process still holds a stale PATH snapshot and wouldn't see
    // them — that's why the wizard reported "could not be installed" right after
    // a successful install (the install worked; only detection was blind). The
    // tools become visible without restarting Operon. No-op on macOS/Linux.
    crate::platform::refresh_path();
    // Delegate to the platform abstraction layer which handles
    // tool discovery paths and Xcode checks per-platform.
    Ok(crate::platform::check_dependencies())
}

/// Refresh the process PATH from the system registry (Windows) or shell profile.
/// Called by the frontend before re-checking dependencies after user installs something.
#[tauri::command]
pub async fn refresh_environment() -> Result<(), String> {
    crate::platform::refresh_path();
    crate::platform::persist_git_bash_env();
    Ok(())
}

/// Install Xcode CLI tools (macOS only, no-op on other platforms).
/// Delegates to the platform abstraction layer.
#[tauri::command]
pub async fn install_xcode_cli() -> Result<(), String> {
    crate::platform::install_xcode_cli_platform()
}

/// The Operon-managed Node.js installation directory.
fn operon_node_dir() -> std::path::PathBuf {
    crate::platform::operon_node_dir()
}

/// Get the path to the Operon-managed `node` binary (if it exists).
fn operon_node_bin() -> Option<String> {
    let dir = operon_node_dir();
    // Unix tarball: <dir>/bin/node. Windows portable zip: <dir>/node.exe at the
    // root (no bin/ subdir). Check both so detection works on every platform.
    for cand in [dir.join("bin").join("node"), dir.join("node.exe")] {
        if cand.exists() {
            return Some(cand.to_string_lossy().to_string());
        }
    }
    None
}

/// Install Node.js using platform-appropriate methods.
/// Delegates to the platform abstraction layer which handles:
/// - macOS: Homebrew → tarball fallback
/// - Windows: winget → tarball
/// - Linux: apt → tarball fallback
#[tauri::command]
pub async fn install_node() -> Result<(), String> {
    crate::platform::install_node_platform()
}

// ── Phased Dependency Installation ──
// Split into 3 phases so the frontend can show separate pages:
//   Phase 1: Xcode CLI Tools (can take 20-30 min on slow internet)
//   Phase 2: Homebrew + Node.js + GitHub CLI
//   Phase 3: Claude Code
//
// Each phase emits `install-progress` events with step/status/message/percent.
// The frontend shows each phase as its own page, with fallback terminal commands on failure.

#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub step: String,   // e.g. "xcode", "homebrew", "node", "gh", "claude", "done"
    pub status: String, // "starting", "downloading", "installing", "waiting", "complete", "skipped", "error"
    pub message: String,
    pub percent: u8, // 0-100 within this phase
}

fn emit_install_progress(
    app: &tauri::AppHandle,
    step: &str,
    status: &str,
    message: &str,
    percent: u8,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: step.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            percent,
        },
    );
}

/// Phase 1: Xcode CLI Tools (macOS only).
/// Triggers the macOS installer dialog and polls until it completes.
/// On non-macOS platforms, this is a no-op that returns true.
#[tauri::command]
pub async fn install_phase_xcode(app: tauri::AppHandle) -> Result<bool, String> {
    if !crate::platform::requires_xcode() {
        emit_install_progress(
            &app,
            "xcode",
            "skipped",
            "Not required on this platform",
            100,
        );
        return Ok(true);
    }

    let already = login_shell_cmd("xcode-select -p")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if already {
        emit_install_progress(
            &app,
            "xcode",
            "skipped",
            "Xcode Command Line Tools already installed",
            100,
        );
        return Ok(true);
    }

    emit_install_progress(
        &app,
        "xcode",
        "starting",
        "Installing Xcode Command Line Tools...",
        5,
    );

    // Delegate to platform layer which calls xcode-select --install
    if let Err(e) = crate::platform::install_xcode_cli_platform() {
        emit_install_progress(
            &app,
            "xcode",
            "error",
            &format!("Xcode install failed: {}", e),
            100,
        );
        return Ok(false);
    }

    emit_install_progress(
        &app,
        "xcode",
        "waiting",
        "A macOS dialog will appear — click Install and wait for it to finish.",
        10,
    );

    // Poll for up to 40 minutes (slow internet scenario)
    for i in 0..480_u32 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let check = login_shell_cmd("xcode-select -p")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if check {
            emit_install_progress(
                &app,
                "xcode",
                "complete",
                "Xcode Command Line Tools installed!",
                100,
            );
            return Ok(true);
        }
        let pct = 10 + std::cmp::min((i * 85 / 480) as u8, 85);
        emit_install_progress(
            &app,
            "xcode",
            "waiting",
            "Waiting for Xcode installer...",
            pct,
        );
    }

    emit_install_progress(
        &app,
        "xcode",
        "error",
        "Xcode install timed out — it may still be running in the background.",
        100,
    );
    Ok(false)
}

/// Phase 2: Package manager + Node.js + GitHub CLI.
/// Uses platform abstraction for package manager detection and installation:
/// - macOS: Homebrew → brew install node/gh
/// - Windows: winget → winget install node/gh
/// - Linux: apt → apt install node/gh
#[tauri::command]
pub async fn install_phase_tools(app: tauri::AppHandle) -> Result<bool, String> {
    let mut all_ok = true;

    // Refresh PATH from the registry first so the "any missing" gate and all
    // detection below see tools a prior run already installed — otherwise a
    // Retry after the elevated batch would re-launch the installer. No-op on Unix.
    crate::platform::refresh_path();

    // ── Windows: NO-ADMIN, fully per-user installs ──
    // Target machines are locked-down lab laptops where the user has no admin
    // rights and cannot answer a UAC prompt. So we never elevate: there is no
    // `Start-Process -Verb RunAs` batch and no `--scope machine`. Every tool is
    // installed into the current user's own profile by the per-tool sections
    // below — Git (per-user installer), Node + gh (portable zips into Operon's
    // data dir, surfaced via extra_tool_paths so no system-PATH write is needed),
    // Python (`winget --scope user`), uv/reportlab/Claude Code (already per-user).
    // `batch_ran` is retained as a constant `false` so the per-tool sections take
    // their normal install path (the old `else if batch_ran` branches are dead).
    let batch_ran = false;

    // ── Git Bash (Windows only, 0-10%) ──
    // Claude Code on Windows requires Git Bash. Install it first so that
    // Claude Code works after npm install.
    #[cfg(target_os = "windows")]
    {
        if crate::platform::find_git_bash_path().is_some() {
            crate::platform::persist_git_bash_env();
            emit_install_progress(&app, "git", "complete", "Git installed", 10);
        } else if batch_ran {
            // The elevated installer already ran — do NOT launch a second Git
            // installer (this was the double-install). Git's installer may still
            // be open/finishing; the user finishes it and clicks Retry.
            emit_install_progress(&app, "git", "error",
                "Git not detected yet — if its installer window is still open, finish it, then click Retry.", 10);
            all_ok = false;
        } else {
            emit_install_progress(
                &app,
                "git",
                "installing",
                "Downloading Git for Windows installer...",
                2,
            );

            match crate::platform::install_git_platform() {
                Ok(()) => {
                    // Shouldn't normally reach here — install_git returns Err sentinels
                    emit_install_progress(&app, "git", "complete", "Git installed!", 10);
                }
                Err(e) if e == "INSTALLER_LAUNCHED" => {
                    emit_install_progress(&app, "git", "error",
                        "Git installer launched — complete the setup wizard, then click Re-check below.", 10);
                    all_ok = false;
                }
                Err(e) if e == "BROWSER_OPENED" => {
                    emit_install_progress(
                        &app,
                        "git",
                        "error",
                        "Download page opened — install Git, then click Re-check below.",
                        10,
                    );
                    all_ok = false;
                }
                Err(e) => {
                    emit_install_progress(
                        &app,
                        "git",
                        "error",
                        &format!(
                            "Git install failed: {}. Download from https://git-scm.com",
                            e
                        ),
                        10,
                    );
                    all_ok = false;
                }
            }
        }

        // Refresh PATH after Git install so subsequent steps find git/node/npm
        crate::platform::refresh_path();
    }

    // ── Package Manager (10-50%) ──
    let mut pkg_mgr = crate::platform::find_package_manager();

    if pkg_mgr.is_none() {
        emit_install_progress(
            &app,
            "homebrew",
            "installing",
            "Installing package manager...",
            5,
        );

        match crate::platform::install_homebrew_platform() {
            Ok(path) => {
                pkg_mgr = Some(path);
                emit_install_progress(
                    &app,
                    "homebrew",
                    "complete",
                    "Package manager installed!",
                    45,
                );
            }
            Err(e) => {
                eprintln!("[Package Manager] Install failed: {}", e);
                emit_install_progress(
                    &app,
                    "homebrew",
                    "error",
                    &format!("Package manager install failed: {}", e),
                    45,
                );
                // Not fatal — Node.js can still be installed via tarball
            }
        }
    } else {
        emit_install_progress(
            &app,
            "homebrew",
            "skipped",
            "Package manager already installed",
            45,
        );
    }

    // ── Node.js (50-80%) ──
    let has_node = login_shell_cmd("node --version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        || operon_node_bin().is_some();

    if !has_node && !batch_ran {
        emit_install_progress(&app, "node", "installing", "Installing Node.js...", 55);

        match crate::platform::install_node_platform() {
            Ok(()) => {
                emit_install_progress(&app, "node", "complete", "Node.js installed!", 80);
            }
            Err(e) => {
                eprintln!("[Node] Install failed: {}", e);
                emit_install_progress(
                    &app,
                    "node",
                    "error",
                    "Node.js could not be installed automatically.",
                    80,
                );
                all_ok = false;
            }
        }
    } else if !has_node {
        // The elevated installer ran but Node isn't detected yet — don't launch
        // a second install; the user finishes any installer window and Retries.
        emit_install_progress(
            &app,
            "node",
            "error",
            "Node.js not detected yet — finish any installer window, then click Retry.",
            80,
        );
        all_ok = false;
    } else {
        let ver = login_shell_cmd("node --version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        emit_install_progress(
            &app,
            "node",
            "skipped",
            &format!("Node.js already installed ({})", ver),
            80,
        );
    }

    // ── GitHub CLI (80-100%) ──
    let has_gh = crate::platform::check_tool("gh").is_some();

    if !has_gh {
        emit_install_progress(&app, "gh", "installing", "Installing GitHub CLI...", 85);
        let mut gh_installed = false;

        // Strategy 1 (Windows): per-user PORTABLE zip into Operon's data dir —
        // NO admin (the winget MSI needs admin, which locked-down accounts lack).
        #[cfg(target_os = "windows")]
        {
            match crate::platform::windows::install_gh() {
                Ok(()) => {
                    crate::platform::refresh_path();
                    gh_installed = true;
                }
                Err(e) => {
                    eprintln!("[gh] portable install failed: {}", e);
                }
            }
        }

        // Strategy 2: Package manager (Homebrew on macOS, apt on Linux)
        if !gh_installed {
            if let Some(ref mgr) = pkg_mgr {
                let output =
                    hide_window(std::process::Command::new(mgr).args(["install", "gh"])).output();
                if let Ok(o) = output {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if o.status.success() || stderr.contains("already installed") {
                        gh_installed = true;
                    } else {
                        eprintln!("[gh] {} install gh failed: {}", mgr, stderr);
                    }
                }
            }
        }

        if gh_installed {
            emit_install_progress(&app, "gh", "complete", "GitHub CLI installed!", 90);
        } else {
            eprintln!("[gh] All install strategies failed");
            emit_install_progress(
                &app,
                "gh",
                "error",
                "GitHub CLI could not be installed (optional — you can install it later).",
                90,
            );
            // gh is optional — don't fail the whole phase
        }
    } else {
        emit_install_progress(&app, "gh", "skipped", "GitHub CLI already installed", 90);
    }

    // ── Python (Windows only, 80-84%) ──
    // On macOS/Linux, Python is typically pre-installed or managed by the user.
    // On Windows, we install it automatically via winget.
    #[cfg(target_os = "windows")]
    {
        if crate::platform::find_python().is_none() {
            emit_install_progress(
                &app,
                "python",
                "installing",
                "Installing Python (required for PDF reports and research tools)...",
                81,
            );

            match crate::platform::install_python_platform() {
                Ok(()) => {
                    emit_install_progress(&app, "python", "complete", "Python installed!", 84);
                }
                Err(e) => {
                    eprintln!("[Python] Install failed: {}", e);
                    emit_install_progress(
                        &app,
                        "python",
                        "error",
                        "Python could not be installed. Install from https://python.org/downloads",
                        84,
                    );
                    // Not fatal — user can install manually
                }
            }
        } else {
            emit_install_progress(&app, "python", "skipped", "Python already installed", 84);
        }
    }

    // ── OpenSSH (Windows only, 84-87%) ──
    // macOS/Linux always have OpenSSH.
    #[cfg(target_os = "windows")]
    {
        // Check both native OpenSSH and Git Bash's ssh
        let has_ssh =
            crate::platform::has_openssh() || crate::platform::find_git_bash_path().is_some(); // Git Bash includes ssh

        if !has_ssh {
            emit_install_progress(
                &app,
                "openssh",
                "installing",
                "Installing OpenSSH client (for remote connections)...",
                85,
            );

            match crate::platform::install_openssh_platform() {
                Ok(()) => {
                    emit_install_progress(&app, "openssh", "complete", "OpenSSH installed!", 87);
                }
                Err(e) => {
                    eprintln!("[OpenSSH] Install failed: {}", e);
                    emit_install_progress(&app, "openssh", "error",
                        "OpenSSH could not be installed (optional — Git Bash provides SSH if needed).", 87);
                    // Not fatal — Git Bash includes ssh as a fallback
                }
            }
        } else {
            emit_install_progress(
                &app,
                "openssh",
                "skipped",
                "SSH available (via OpenSSH or Git Bash)",
                87,
            );
        }
    }

    // ── uv / uvx (Windows only, 87-90%) ──
    // On macOS/Linux, uv is typically installed via brew or curl by the user.
    // On Windows, we install it automatically.
    #[cfg(target_os = "windows")]
    {
        if !crate::platform::has_uv() {
            emit_install_progress(
                &app,
                "uv",
                "installing",
                "Installing uv (Python package manager for research tools)...",
                88,
            );

            match crate::platform::install_uv_platform() {
                Ok(()) => {
                    emit_install_progress(&app, "uv", "complete", "uv installed!", 90);
                }
                Err(e) => {
                    eprintln!("[uv] Install failed: {}", e);
                    emit_install_progress(
                        &app,
                        "uv",
                        "error",
                        "uv could not be installed. Install from https://docs.astral.sh/uv/",
                        90,
                    );
                }
            }
        } else {
            emit_install_progress(&app, "uv", "skipped", "uv already installed", 90);
        }
    }

    // ── Python reportlab for PDF reports (90-100%) ──
    let has_reportlab = crate::platform::has_reportlab();

    if !has_reportlab {
        emit_install_progress(
            &app,
            "reportlab",
            "installing",
            "Installing PDF report library (reportlab)...",
            92,
        );

        match crate::platform::install_reportlab_platform() {
            Ok(()) => {
                emit_install_progress(&app, "reportlab", "complete", "reportlab installed!", 100);
            }
            Err(_e) => {
                emit_install_progress(
                    &app,
                    "reportlab",
                    "error",
                    "reportlab could not be installed (Report mode will install it on first use).",
                    100,
                );
                // Don't fail the whole phase — report mode has its own fallback
            }
        }
    } else {
        emit_install_progress(
            &app,
            "reportlab",
            "skipped",
            "reportlab already installed",
            100,
        );
    }

    emit_install_progress(
        &app,
        "done",
        if all_ok { "complete" } else { "error" },
        if all_ok {
            "All tools installed!"
        } else {
            "Some items need attention"
        },
        100,
    );

    Ok(all_ok)
}

/// Phase 3: Claude Code.
/// Delegates to the platform abstraction layer which handles:
/// - macOS: curl installer → npm fallback → Terminal.app fallback
/// - Windows: curl installer via Git Bash → npm fallback
/// - Linux: curl installer → npm fallback
#[tauri::command]
pub async fn install_phase_claude(app: tauri::AppHandle) -> Result<bool, String> {
    // Refresh PATH so we can find npm/claude from tools installed in previous phases
    crate::platform::refresh_path();

    // On Windows, ensure CLAUDE_CODE_GIT_BASH_PATH is set before installing/running Claude
    crate::platform::persist_git_bash_env();

    // Check Git Bash is available on Windows before proceeding
    #[cfg(target_os = "windows")]
    {
        if crate::platform::find_git_bash_path().is_none() {
            emit_install_progress(&app, "claude", "error",
                "Git Bash is required but not installed. Please install Git from https://git-scm.com/downloads/win and restart Operon.", 100);
            return Ok(false);
        }
    }

    let has_claude = crate::platform::check_tool("claude").is_some();

    if has_claude {
        let ver = login_shell_cmd("claude --version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        emit_install_progress(
            &app,
            "claude",
            "skipped",
            &format!("Claude Code already installed ({})", ver),
            100,
        );
        return Ok(true);
    }

    emit_install_progress(
        &app,
        "claude",
        "installing",
        "Installing Claude Code...",
        20,
    );

    match crate::platform::install_claude_platform() {
        Ok(()) => {
            emit_install_progress(&app, "claude", "complete", "Claude Code installed!", 100);
            Ok(true)
        }
        Err(e) => {
            eprintln!("[Claude] Platform install failed: {}", e);
            let hint = if cfg!(target_os = "windows") {
                "Claude Code could not be installed automatically. Try running: npm install -g @anthropic-ai/claude-code"
            } else {
                "Claude Code could not be installed automatically. Try running: curl -fsSL https://claude.ai/install.sh | bash"
            };
            emit_install_progress(&app, "claude", "error", hint, 100);
            Ok(false)
        }
    }
}

/// Legacy wrapper — calls all 3 phases sequentially.
/// Kept for backward compatibility if anything still calls it.
#[tauri::command]
pub async fn install_all_dependencies(app: tauri::AppHandle) -> Result<(), String> {
    install_phase_xcode(app.clone()).await?;
    install_phase_tools(app.clone()).await?;
    install_phase_claude(app).await?;
    Ok(())
}

/// Check if Claude Code is available on a remote server via SSH
#[tauri::command]
pub async fn check_remote_claude(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<DependencyStatus, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("TRANSIENT: SSH profile {} not found", profile_id))?
    };

    // Check all deps in one SSH call for efficiency.
    // Check multiple locations: PATH, ~/.npm-global/bin, ~/.claude/local/bin
    let check_script = r#"
# Add common install locations to PATH
export PATH="$HOME/.npm-global/bin:$HOME/.claude/local/bin:$HOME/.local/bin:$PATH"

echo "NODE:$(node --version 2>/dev/null || echo MISSING)"
echo "NPM:$(npm --version 2>/dev/null || echo MISSING)"

# Check claude — look in PATH, official install dir, npm-global, and shell profiles
CLAUDE_VER="MISSING"
if command -v claude &>/dev/null; then
  CLAUDE_VER="$(claude --version 2>/dev/null || echo FOUND)"
elif [ -x "$HOME/.claude/local/bin/claude" ]; then
  CLAUDE_VER="$($HOME/.claude/local/bin/claude --version 2>/dev/null || echo FOUND)"
elif [ -x "$HOME/.npm-global/bin/claude" ]; then
  CLAUDE_VER="$($HOME/.npm-global/bin/claude --version 2>/dev/null || echo FOUND)"
elif [ -f ~/.bashrc ] || [ -f ~/.bash_profile ]; then
  # Last resort: alias-based installs (e.g. `alias claude='npx ...'`). Safe to
  # source profiles here — the whole check runs inside the channel's `( … )`
  # subshell, so any profile `exit` or PATH change is discarded with the subshell
  # and can't affect the persistent parent shell.
  export PS1=x
  shopt -s expand_aliases 2>/dev/null
  source ~/.bashrc 2>/dev/null
  source ~/.bash_profile 2>/dev/null
  if command -v claude &>/dev/null || alias claude &>/dev/null 2>&1; then
    CLAUDE_VER="$(claude --version 2>/dev/null || echo FOUND)"
  fi
fi
echo "CLAUDE:$CLAUDE_VER"
echo "REPORTLAB:$(python3 -c 'import reportlab; print(reportlab.Version)' 2>/dev/null || echo MISSING)"
"#;

    // Any failure from ssh_exec_async at this stage is a connection / auth /
    // transport problem (we never even reached the remote claude binary). Tag
    // it with TRANSIENT: so the frontend can render a "Reconnect SSH" banner
    // instead of misleading the user into reinstalling Claude Code.
    let result = super::ssh::ssh_exec_async(profile, check_script.to_string())
        .await
        .map_err(|e| format!("TRANSIENT: SSH check failed: {}", e))?;

    let node_line = result
        .lines()
        .find(|l| l.starts_with("NODE:"))
        .unwrap_or("NODE:MISSING");
    let npm_line = result
        .lines()
        .find(|l| l.starts_with("NPM:"))
        .unwrap_or("NPM:MISSING");
    let claude_line = result
        .lines()
        .find(|l| l.starts_with("CLAUDE:"))
        .unwrap_or("CLAUDE:MISSING");
    let reportlab_line = result
        .lines()
        .find(|l| l.starts_with("REPORTLAB:"))
        .unwrap_or("REPORTLAB:MISSING");
    let _reportlab_ver = reportlab_line
        .strip_prefix("REPORTLAB:")
        .unwrap_or("MISSING");
    // reportlab status is logged but not yet surfaced in DependencyStatus

    let node_ver = node_line.strip_prefix("NODE:").unwrap_or("MISSING");
    let npm_ver = npm_line.strip_prefix("NPM:").unwrap_or("MISSING");
    let claude_ver = claude_line.strip_prefix("CLAUDE:").unwrap_or("MISSING");

    Ok(DependencyStatus {
        xcode_cli: true, // Not applicable for remote
        node: node_ver != "MISSING",
        node_version: if node_ver != "MISSING" {
            Some(node_ver.to_string())
        } else {
            None
        },
        npm: npm_ver != "MISSING",
        npm_version: if npm_ver != "MISSING" {
            Some(npm_ver.to_string())
        } else {
            None
        },
        claude_code: claude_ver != "MISSING",
        claude_version: if claude_ver != "MISSING" && claude_ver != "FOUND" {
            Some(claude_ver.to_string())
        } else {
            None
        },
        git_bash: true, // Not applicable for remote (Linux servers)
    })
}

/// Latest published Claude Code version from the npm registry (`dist-tags.latest`
/// via the `/latest` document's `version`). The remote-deps panel compares this
/// to the server's installed version to show "up to date" / "update available".
/// Best-effort — any network error is returned as Err and the UI just omits the
/// hint. Runs from the app host (not the remote), so it works behind a login
/// node with no outbound access.
#[tauri::command]
pub async fn get_latest_claude_code_version() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("operon")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get("https://registry.npmjs.org/@anthropic-ai/claude-code/latest")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("npm registry returned {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    json.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "no version field in npm response".to_string())
}

/// Run `claude update` (the native installer's self-update) on a remote server,
/// trying the same install locations `check_remote_claude` probes. Returns the
/// combined output for display; the caller re-checks the version afterwards. The
/// update is a brief, user-initiated binary refresh (not a long-running agent),
/// so it's safe even on login-node-restricted clusters.
#[tauri::command]
pub async fn update_remote_claude(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };
    let script = r#"
export PATH="$HOME/.local/bin:$HOME/.claude/local/bin:$HOME/.npm-global/bin:$PATH"
if command -v claude >/dev/null 2>&1; then
  claude update 2>&1
elif [ -x "$HOME/.local/bin/claude" ]; then
  "$HOME/.local/bin/claude" update 2>&1
elif [ -x "$HOME/.claude/local/bin/claude" ]; then
  "$HOME/.claude/local/bin/claude" update 2>&1
else
  echo "claude not found on PATH — update manually (see docs)"; exit 1
fi
"#;
    super::ssh::ssh_exec_async(profile, script.to_string())
        .await
        .map_err(|e| e.to_string())
}

/// Check if Claude Code on a remote server is authenticated.
/// Reads the stored OAuth credential (`expiresAt` + `refreshToken`) directly —
/// it does NOT run `claude`, because a live `claude -p` would force a token
/// REFRESH, and on a shared-NFS HPC home that refresh can race a session/other
/// check and rotate-invalidate (even permanently wipe) the refresh token. The
/// file read tells us auth status without ever touching the network.
/// Returns: "authenticated", "not_authenticated", or an error string.
#[tauri::command]
pub async fn check_remote_claude_auth(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Determine auth status by READING the stored OAuth credential — NEVER by
    // running `claude`. A live `claude -p` would force a token REFRESH; on HPC
    // the credential is on a shared NFS home, and a refresh racing another
    // claude process (a session, or the old 5s re-check) rotates + invalidates
    // the refresh token, which a lost race can permanently wipe. Reading the
    // file's `expiresAt` + `refreshToken` tells us enough without any network
    // call and without ever triggering that refresh. NO `claude`, NO python.
    let check_script = r#"
# Locate the credential file (first match wins).
CRED=""
for f in \
    "$HOME/.claude/.credentials.json" \
    "$HOME/.claude/credentials.json" \
    "$HOME/.claude/.credentials" \
    "$HOME/.claude.json" \
    "$HOME/.config/claude/credentials.json" \
    "$HOME/.config/claude-code/credentials.json"
do
    if [ -s "$f" ]; then CRED="$f"; break; fi
done
# Fallback: any non-empty hidden json in ~/.claude/
if [ -z "$CRED" ]; then
    for f in "$HOME/.claude"/.*.json; do
        [ -s "$f" ] 2>/dev/null && { CRED="$f"; break; }
    done
fi

if [ -z "$CRED" ]; then
    echo "AUTH:none"
    ls -la "$HOME/.claude/" 2>&1 | head -20 | while read line; do echo "DEBUG:$line"; done
    exit 0
fi

# Parse expiresAt (epoch ms) and refreshToken using grep/sed only. Scanning the
# whole blob handles BOTH the wrapped {claudeAiOauth:{...}} and flat schemas and
# tolerates pretty-printed JSON.
RAW=$(cat "$CRED" 2>/dev/null)
EXP_MS=$(printf '%s' "$RAW" | grep -o '"expiresAt"[[:space:]]*:[[:space:]]*[0-9][0-9]*' | grep -o '[0-9][0-9]*$' | head -1)
RT=$(printf '%s' "$RAW" | sed -n 's/.*"refreshToken"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)

# ms -> s WITHOUT 64-bit arithmetic inside [ ] (strip the trailing 3 ms digits).
# Treat a missing/short/non-numeric expiresAt as "unparsable" (EXP_S empty) so we
# fall through to the refresh-token check rather than a false logout.
EXP_S=""
case "$EXP_MS" in
    "" ) ;;
    *[!0-9]* ) ;;
    ??? | ?? | ? ) ;;
    * ) EXP_S=${EXP_MS%???} ;;
esac
NOW_S=$(date +%s 2>/dev/null)

echo "DEBUG:auth file=$CRED exp_ms=${EXP_MS:-none} has_rt=$([ -n "$RT" ] && echo yes || echo no)"

if [ -n "$EXP_S" ] && [ -n "$NOW_S" ] && [ "$EXP_S" -gt "$NOW_S" ] 2>/dev/null; then
    # Access token still valid by its own expiry — no refresh needed.
    echo "AUTH:verified"
elif [ -n "$RT" ]; then
    # Expired (or expiry unparsable) but a refresh token is present — Claude Code
    # will silently refresh on the next REAL use. Treat as ok; do NOT refresh here.
    echo "AUTH:ok"
else
    # Expired/unknown AND no refresh token -> genuinely needs re-login.
    echo "AUTH:expired"
    echo "DEBUG:access token expired/unparsable and no refresh token present"
fi
exit 0
"#;

    let result = super::ssh::ssh_exec_async(profile, check_script.to_string())
        .await
        .map_err(|e| format!("SSH auth check failed: {}", e))?;

    eprintln!("[Operon] Remote auth check result: {}", result.trim());

    if result.contains("AUTH:verified") || result.contains("AUTH:ok") {
        Ok("authenticated".to_string())
    } else if result.contains("AUTH:expired") {
        // Credential files exist but are expired/invalid
        Ok(format!(
            "not_authenticated:credentials_expired:{}",
            result.trim()
        ))
    } else {
        // No credentials found at all
        Ok(format!("not_authenticated:{}", result.trim()))
    }
}

/// Install Claude Code on a remote server via SSH.
/// On HPC servers users typically don't have sudo, so we configure npm
/// to use a user-local prefix (~/.npm-global) and install there.
#[tauri::command]
pub async fn install_remote_claude(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Use the official Claude Code installer (no Node.js dependency).
    // Falls back to npm if curl installer fails.
    let install_script = "
# Ensure common install locations are in PATH
export PATH=\"$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH\"

# Method 1: Official Claude Code installer (recommended, no Node.js needed)
echo '>>> Installing Claude Code via official installer...'
if command -v curl >/dev/null 2>&1; then
    curl -fsSL https://claude.ai/install.sh | bash 2>&1
    # Source updated profile so claude is in PATH. Redirect stdout too (not just
    # stderr) so MOTD/conda banners don't land in the channel and corrupt the
    # parsed install result.
    [ -f $HOME/.bashrc ] && . $HOME/.bashrc >/dev/null 2>&1
    [ -f $HOME/.bash_profile ] && . $HOME/.bash_profile >/dev/null 2>&1
    [ -f $HOME/.profile ] && . $HOME/.profile >/dev/null 2>&1
fi

# Check if it worked
if command -v claude >/dev/null 2>&1; then
    echo OPERON_INSTALL_SUCCESS
    claude --version 2>/dev/null || echo installed
    exit 0
fi

# Also check ~/.claude/local/bin (common install location)
if [ -x $HOME/.claude/local/bin/claude ]; then
    echo OPERON_INSTALL_SUCCESS
    $HOME/.claude/local/bin/claude --version 2>/dev/null || echo installed
    exit 0
fi

# Method 2: npm fallback (if Node.js is available)
if command -v npm >/dev/null 2>&1; then
    echo '>>> Curl installer did not work, trying npm fallback...'
    NPM_PREFIX=$HOME/.npm-global
    mkdir -p $NPM_PREFIX
    npm config set prefix $NPM_PREFIX 2>&1
    export PATH=$NPM_PREFIX/bin:$PATH
    npm install -g @anthropic-ai/claude-code 2>&1

    # Persist PATH
    LINE='export PATH=$HOME/.npm-global/bin:$PATH'
    for rc in $HOME/.bashrc $HOME/.bash_profile $HOME/.profile; do
        if [ -f $rc ]; then
            if ! grep -q .npm-global/bin $rc 2>/dev/null; then
                echo '' >> $rc
                echo '# Added by Operon - npm user-local bin' >> $rc
                echo $LINE >> $rc
            fi
        fi
    done

    if command -v claude >/dev/null 2>&1 || [ -x $NPM_PREFIX/bin/claude ]; then
        echo OPERON_INSTALL_SUCCESS
        claude --version 2>/dev/null || $NPM_PREFIX/bin/claude --version 2>/dev/null || echo installed
        exit 0
    fi
fi

echo OPERON_INSTALL_FAILED
";

    let result = super::ssh::ssh_exec(&profile, install_script)
        .map_err(|e| format!("Remote install failed: {}", e))?;

    if result.contains("OPERON_INSTALL_SUCCESS") {
        // Probe for the actual claude binary path and store it in server_config
        let probe_script = r#"
export PATH="$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH"
for p in "$HOME/.claude/local/bin/claude" "$HOME/.local/bin/claude" "$HOME/.npm-global/bin/claude"; do
    [ -x "$p" ] && { echo "CLAUDE_PATH:$p"; exit 0; }
done
# fallback: which claude
WHICH=$(command -v claude 2>/dev/null)
[ -n "$WHICH" ] && echo "CLAUDE_PATH:$WHICH" || echo "CLAUDE_PATH:claude"
"#;
        if let Ok(probe_result) = super::ssh::ssh_exec(&profile, probe_script) {
            if let Some(path_line) = probe_result.lines().find(|l| l.starts_with("CLAUDE_PATH:")) {
                let claude_path = path_line
                    .strip_prefix("CLAUDE_PATH:")
                    .unwrap_or("claude")
                    .trim()
                    .to_string();
                eprintln!("[operon] Detected remote claude path: {}", claude_path);
                // Store in server_config
                {
                    let mut profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                    if let Some(prof) = profiles.iter_mut().find(|p| p.id == profile_id) {
                        prof.server_config
                            .insert("claude_path".to_string(), claude_path);
                        let _ = super::ssh::save_profiles_to_disk(&profiles);
                    }
                }
            }
        }

        // Ensure PATH is in all common shell rc files so `claude` works in interactive terminals
        let patch_rc_script = r#"
LINE='export PATH="$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH"'
MARKER='# Added by Operon'
for rc in "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile" "$HOME/.zshrc"; do
    [ -f "$rc" ] || continue
    grep -q '.claude/local/bin\|.local/bin.*claude\|.npm-global/bin' "$rc" 2>/dev/null && continue
    printf '\n%s\n%s\n' "$MARKER" "$LINE" >> "$rc"
done
"#;
        let _ = super::ssh::ssh_exec(&profile, patch_rc_script);

        // Also install reportlab for PDF report generation on the remote server
        let reportlab_script = r#"
if python3 -c 'import reportlab' 2>/dev/null; then
    echo 'REPORTLAB_OK'
else
    echo '>>> Installing reportlab for PDF reports...'
    python3 -m pip install reportlab --user --quiet 2>/dev/null \
        || python3 -m pip install reportlab --quiet --break-system-packages 2>/dev/null \
        || pip3 install reportlab --user --quiet 2>/dev/null \
        || echo 'REPORTLAB_SKIP'
    if python3 -c 'import reportlab' 2>/dev/null; then
        echo 'REPORTLAB_OK'
    else
        echo 'REPORTLAB_SKIP'
    fi
fi
"#;
        // Best-effort: don't fail the whole install if reportlab can't be installed
        if let Ok(rl_result) = super::ssh::ssh_exec(&profile, reportlab_script) {
            if rl_result.contains("REPORTLAB_SKIP") {
                eprintln!("[operon] reportlab could not be installed on remote server — report mode will attempt at runtime");
            }
        }
        return Ok(());
    }

    // Provide a helpful error with manual install command
    Err(format!(
        "Automatic installation failed on this server.\n\n\
         You can install manually by running this in the terminal:\n  \
         curl -fsSL https://claude.ai/install.sh | bash\n\n\
         Then click Re-check in Operon.\n\n\
         Server output:\n{}",
        result.lines().take(20).collect::<Vec<_>>().join("\n")
    ))
}

// --- Remote OAuth Login ---

/// Run `claude login` on a remote server via SSH, extract the OAuth URL,
/// and return it so the frontend can display it as a clickable link.
/// After the user authenticates in their browser, they paste the code back,
/// which is sent via `remote_claude_login_code`.
#[tauri::command]
pub async fn remote_claude_login(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Resolve the claude binary path from server_config or fall back to common locations
    let claude_bin = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .and_then(|p| p.server_config.get("claude_path").cloned())
            .unwrap_or_else(|| "claude".to_string())
    };

    // Run `claude login` on the remote server and extract the OAuth URL.
    //
    // `claude login` is interactive and needs a PTY to produce output. We use
    // the `script` command (available on all Linux/macOS) to allocate a pseudo-TTY.
    // The process runs in the background so it stays alive to receive the OAuth
    // callback after the user authenticates in their browser.
    //
    // Strategy:
    //   1. Use `script` to run `claude login` with a PTY, output to a temp file
    //   2. Background it so it stays alive for the OAuth callback
    //   3. Poll the temp file for up to 30s until the OAuth URL appears
    //   4. Strip ANSI escape codes when extracting the URL
    let login_script = format!(
        r#"{prefix}
LOGFILE="/tmp/.operon-login-$$.log"
rm -f "$LOGFILE"
touch "$LOGFILE"

# Use `script` to provide a pseudo-TTY for claude login, which needs
# interactive output to display the OAuth URL.
# Linux (util-linux) and macOS/BSD have different `script` flag syntax.
if script -V 2>&1 | grep -qi 'util-linux' || [ "$(uname)" = "Linux" ]; then
  # Linux: script -q -c 'cmd' outfile
  script -q -c 'TERM=dumb {claude_bin} login 2>&1' "$LOGFILE" </dev/null &
else
  # macOS / BSD: script -q outfile cmd...
  script -q "$LOGFILE" bash -c 'TERM=dumb {claude_bin} login 2>&1' </dev/null &
fi
LOGIN_PID=$!

# Poll for URL to appear (up to 30 seconds)
for i in $(seq 1 60); do
  # Strip ANSI escape codes and carriage returns, then look for https URL
  URL=$(sed 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/\x1b\][^\x07]*\x07//g; s/\x1b[()][A-Z0-9]//g; s/\r//g' "$LOGFILE" 2>/dev/null | tr -d '\000' | grep -oE 'https://[^[:space:]"'"'"']+' | head -1)
  if [ -n "$URL" ]; then
    echo "URL:$URL"
    echo "PID:$LOGIN_PID"
    echo "LOG:$LOGFILE"
    exit 0
  fi
  sleep 0.5
done

# Timeout — dump whatever we got (strip ANSI for readability)
echo "TIMEOUT"
CLEANED=$(sed 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/\x1b\][^\x07]*\x07//g; s/\x1b[()][A-Z0-9]//g; s/\r//g' "$LOGFILE" 2>/dev/null | tr -d '\000' | head -30)
if [ -n "$CLEANED" ]; then
  echo "$CLEANED"
else
  echo "(no output captured — script command may not be available)"
  # Last-resort fallback: try without PTY
  TERM=dumb timeout 10 {claude_bin} login 2>&1 | head -30 || echo "(direct login also failed)"
fi
"#,
        prefix = REMOTE_PATH_PREFIX,
        claude_bin = claude_bin,
    );

    let result = super::ssh::ssh_exec(&profile, &login_script)
        .map_err(|e| format!("Failed to run claude login: {}", e))?;

    eprintln!("[operon] remote_claude_login output: {}", result);

    // Extract the URL from our structured output
    let url = result.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("URL:").map(|s| s.to_string())
    });

    // If structured extraction failed, try broad URL scan as fallback
    let url = url.or_else(|| {
        result
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if let Some(start) = line.find("https://") {
                    let url_part = &line[start..];
                    let end = url_part
                        .find(|c: char| c.is_whitespace())
                        .unwrap_or(url_part.len());
                    Some(url_part[..end].to_string())
                } else {
                    None
                }
            })
            .find(|url| url.contains("claude.ai") || url.contains("anthropic.com"))
    });

    match url {
        Some(url) => Ok(url),
        None => {
            // Check for common failure reasons
            let hint = if result.contains("TIMEOUT") {
                "The login command timed out waiting for an OAuth URL — the server may be slow or `claude login` may require a different setup."
            } else if result.contains("command not found") || result.contains("not found") {
                "Claude Code may not be installed or not in PATH on this server."
            } else if result.contains("permission denied") || result.contains("Permission denied") {
                "Permission denied — check that you have access to run Claude on this server."
            } else if result.is_empty() || result.trim().is_empty() {
                "No output was returned — the SSH connection may have dropped."
            } else {
                "The login command did not produce an authentication URL."
            };
            Err(format!(
                "{}\n\nYou can log in manually by running 'claude login' in a terminal on the server.\n\nServer output:\n{}",
                hint,
                result.lines().take(10).collect::<Vec<_>>().join("\n")
            ))
        }
    }
}

// --- Authentication ---

#[tauri::command]
pub async fn store_api_key(
    state: tauri::State<'_, ClaudeManager>,
    key: String,
) -> Result<(), String> {
    let mut api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    *api_key = Some(key);
    // In production, use keyring crate for macOS Keychain storage
    Ok(())
}

#[tauri::command]
pub async fn get_api_key(state: tauri::State<'_, ClaudeManager>) -> Result<Option<String>, String> {
    let api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    Ok(api_key.clone())
}

#[tauri::command]
pub async fn delete_api_key(state: tauri::State<'_, ClaudeManager>) -> Result<(), String> {
    let mut api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    *api_key = None;
    Ok(())
}

/// How the frontend should invoke `claude`, and whether it exists at all.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeInvocation {
    /// False means Claude Code could not be found anywhere — the caller must
    /// offer an install step, not a login terminal.
    pub resolved: bool,
    /// Shell-ready command word: an absolute, quoted path where we can resolve
    /// one, else the bare name.
    pub command: String,
}

/// Resolve the `claude` command word for the frontend.
///
/// The terminal the login flow types into is an interactive NON-login shell, so
/// a bare `claude` can fail there even when Claude Code is installed (see
/// `commands/terminal.rs`). Pinning the absolute path removes that whole class
/// of failure, and `resolved: false` is the signal to show an install step
/// instead of a login prompt the user can never satisfy.
#[tauri::command]
pub async fn get_claude_invocation() -> Result<ClaudeInvocation, String> {
    // `check_tool` is the broadest detector — login-shell `which` first, then a
    // direct probe of the known install dirs — so it decides "is it installed?".
    // The command word comes from `claude_invocation()`, the same resolver the
    // chat spawn uses, so login and chat can never disagree about which binary
    // they mean. On Windows that is deliberately the bare name: Git Bash cannot
    // exec a `.cmd` shim by absolute path.
    Ok(ClaudeInvocation {
        resolved: crate::platform::check_tool("claude").is_some(),
        command: crate::platform::claude_invocation(),
    })
}

/// Credential files the Claude Code CLI has used across versions.
///
/// Matched by exact path rather than a filename substring. The previous check
/// accepted any file in `~/.claude/` whose NAME contained "auth", "token",
/// "credential" or "oauth" and whose body was merely non-empty — so
/// `mcp-needs-auth-cache.json`, a cache of which MCP servers need authorization,
/// was enough to report the user as signed in to their Claude subscription.
fn oauth_credential_files() -> Vec<std::path::PathBuf> {
    let Some(home) = crate::platform::home_dir() else {
        return Vec::new();
    };
    vec![
        home.join(".claude/.credentials.json"),
        home.join(".claude/credentials.json"),
        home.join(".config/claude/credentials.json"),
        home.join(".config/claude-code/credentials.json"),
    ]
}

/// True if `v` carries a non-empty OAuth access/refresh token anywhere inside it.
///
/// Matched on content, not on shape: the CLI has moved this field between
/// versions (top level, later nested under `claudeAiOauth`), and a structural
/// match would silently start returning false after an upgrade.
fn json_has_oauth_material(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Object(map) => map.iter().any(|(k, val)| {
            let key = k.to_ascii_lowercase().replace(['_', '-'], "");
            let is_token_key = key.contains("accesstoken") || key.contains("refreshtoken");
            let has_value = val.as_str().is_some_and(|s| !s.trim().is_empty());
            (is_token_key && has_value) || json_has_oauth_material(val)
        }),
        serde_json::Value::Array(items) => items.iter().any(json_has_oauth_material),
        _ => false,
    }
}

/// macOS stores the CLI's OAuth credential in the login Keychain, not on disk —
/// so a file-only check reports a perfectly good subscription login as absent.
///
/// Existence only: `security find-generic-password` WITHOUT `-w` returns
/// attributes and never reads the secret, so it cannot raise a keychain-access
/// prompt. Exit 0 = present, 44 = not found.
#[cfg(target_os = "macos")]
fn keychain_oauth_credential_present() -> bool {
    std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn keychain_oauth_credential_present() -> bool {
    false
}

/// Fast, side-effect-free check for a stored OAuth credential, across both
/// stores the CLI actually uses.
fn stored_oauth_credential_present() -> bool {
    if keychain_oauth_credential_present() {
        return true;
    }
    oauth_credential_files().iter().any(|p| {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .is_some_and(|v| json_has_oauth_material(&v))
    })
}

/// Check if the user has an active OAuth session via Claude CLI.
/// Fast path: look for a stored credential in the Keychain (macOS) or in one of
/// the CLI's known credential files. If neither is present, fall back to running
/// `claude` through a login shell to test whether auth actually works.
#[tauri::command]
pub async fn check_oauth_status() -> Result<bool, String> {
    if stored_oauth_credential_present() {
        return Ok(true);
    }

    // Slow path: actually run claude through a login shell to test auth.
    // Clear EVERY credential var first so we test the NATIVE OAuth credential:
    //  - a leftover `ANTHROPIC_BASE_URL` (Portkey / custom) would route this ping
    //    through a gateway to Bedrock, where newer Claude Code beta headers 400
    //    ("invalid beta flag") and make a valid login look unauthenticated;
    //  - a leftover `ANTHROPIC_API_KEY` is worse in the other direction — the
    //    ping succeeds on the API key and we report the OAuth login as healthy
    //    when the real chat session will fail with "Invalid API key". That
    //    false positive is why this class of bug kept looking fixed.
    // `claude` is pinned to its absolute path for the same reason as the chat
    // spawn: a Finder/Dock launch has no ~/.local/bin on PATH.
    // Substitute BEFORE prepending the `unset`: the rewrite only anchors at the
    // start of the string (or after a pipe), so it must see a bare `claude ...`.
    let probe = substitute_claude_invocation(
        "claude -p 'ping' --max-turns 1 --output-format json 2>/dev/null",
    );
    let auth_probe = format!("unset {}; {}", MANAGED_AUTH_VARS.join(" "), probe);
    let mut auth_cmd = crate::platform::shell_exec_async(&auth_probe);
    if let Some(git_bash_path) = crate::platform::find_git_bash_path() {
        auth_cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
    }
    let output = auth_cmd.output().await.map_err(|e| e.to_string())?;

    // If claude exits 0 and produces output, auth is working
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Launch `claude login` and open the OAuth URL in the user's browser.
///
/// Runs `claude login` as a direct child process with CLAUDE_CODE_GIT_BASH_PATH
/// set (Windows). Captures stdout/stderr, extracts the OAuth URL, and opens it
/// in the default browser. This avoids the fragile "open external terminal" approach
/// which fails when PowerShell/wt aren't in PATH or when env vars don't propagate.
#[tauri::command]
pub async fn launch_claude_login() -> Result<String, String> {
    // Refresh PATH so we can find the claude binary
    crate::platform::refresh_path();
    // Ensure Git Bash env var is set
    crate::platform::persist_git_bash_env();

    // --- Strategy 1: Run `claude login` directly and capture the OAuth URL ---
    // Find the actual claude binary path for reliable execution
    let claude_path = crate::platform::check_tool("claude").map(|(path, _)| path);

    let augmented = crate::platform::augmented_path();

    // Build the command — try direct path first, fall back to shell.
    // spawn_resolve wraps a Windows `.cmd`/`.bat` shim (npm-installed claude)
    // as ("cmd.exe", ["/c", path]) — Command::new can't execute a batch file
    // directly. A real `.exe` / unix binary comes back as (path, []) unchanged.
    let mut cmd = if let Some(ref path) = claude_path {
        let (program, prefix_args) = crate::platform::spawn_resolve(path);
        let mut c = tokio::process::Command::new(&program);
        c.args(&prefix_args);
        c.arg("login");
        c
    } else {
        crate::platform::shell_exec_async("claude login")
    };

    // Set environment for Claude Code
    cmd.env("PATH", &augmented);
    if let Some(git_bash_path) = crate::platform::find_git_bash_path() {
        cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
    }
    // Log in against claude.ai, not against whatever credential happens to be in
    // the environment. With an ANTHROPIC_API_KEY set, `claude login` reports that
    // connectors are disabled because the key takes precedence — the login then
    // appears to succeed but is never the credential actually used.
    for var in MANAGED_AUTH_VARS {
        cmd.env_remove(var);
    }
    // Prevent claude from trying to open a browser itself (we'll do it)
    cmd.env("BROWSER", "echo");

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    hide_window_async(&mut cmd);

    let child = cmd.spawn();

    if let Ok(mut child) = child {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let url_found = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let url_clone = url_found.clone();

        // Read stdout and stderr looking for the OAuth URL
        tokio::spawn(async move {
            let mut all_output = String::new();

            // Helper to extract OAuth URL from text
            fn extract_url(text: &str) -> Option<String> {
                for line in text.lines() {
                    let trimmed = line.trim();
                    // Direct URL line
                    if trimmed.starts_with("https://") {
                        let url = trimmed.split_whitespace().next().unwrap_or(trimmed);
                        if url.contains("oauth")
                            || url.contains("claude.ai")
                            || url.contains("anthropic")
                            || url.contains("login")
                        {
                            return Some(url.to_string());
                        }
                    }
                    // "Open this URL: https://..." or similar
                    if let Some(url_start) = trimmed.find("https://") {
                        let url_part = &trimmed[url_start..];
                        let url = url_part.split_whitespace().next().unwrap_or(url_part);
                        if url.contains("claude.ai")
                            || url.contains("oauth")
                            || url.contains("anthropic")
                            || url.contains("login")
                        {
                            return Some(url.to_string());
                        }
                    }
                }
                None
            }

            // Read stdout
            let stdout_task = async {
                let mut buf = String::new();
                if let Some(mut s) = stdout {
                    use tokio::io::AsyncReadExt;
                    let mut bytes = vec![0u8; 4096];
                    loop {
                        match s.read(&mut bytes).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let chunk = String::from_utf8_lossy(&bytes[..n]);
                                buf.push_str(&chunk);
                                if let Some(url) = extract_url(&chunk) {
                                    return (buf, Some(url));
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                (buf, Option::<String>::None)
            };

            // Read stderr
            let stderr_task = async {
                let mut buf = String::new();
                if let Some(mut s) = stderr {
                    use tokio::io::AsyncReadExt;
                    let mut bytes = vec![0u8; 4096];
                    loop {
                        match s.read(&mut bytes).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let chunk = String::from_utf8_lossy(&bytes[..n]);
                                buf.push_str(&chunk);
                                if let Some(url) = extract_url(&chunk) {
                                    return (buf, Some(url));
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                (buf, Option::<String>::None)
            };

            let ((out, out_url), (err, err_url)) = tokio::join!(stdout_task, stderr_task);

            all_output.push_str(&out);
            all_output.push_str(&err);

            // Use the first URL found from either stream
            let found_url: Option<String> =
                out_url.or(err_url).or_else(|| extract_url(&all_output));

            if let Some(ref url) = found_url {
                let _ = crate::platform::open_url(url);
                if let Ok(mut u) = url_clone.lock() {
                    *u = url.clone();
                }
            }

            eprintln!(
                "[Claude Login] Output: {}",
                all_output.chars().take(500).collect::<String>()
            );
        });

        // Give the process a moment to output the URL
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;

        let found = url_found.lock().map(|u| !u.is_empty()).unwrap_or(false);
        if found {
            return Ok(
                "Login page opened in your browser. Complete the sign-in, then click Verify below."
                    .to_string(),
            );
        }
    }

    // --- Strategy 2: Open an external terminal with `claude login` ---
    // The external terminal sources the user's profile, so the same stale
    // credential we cleared above comes right back — clear it in the command
    // itself. POSIX only: on Windows this lands in cmd.exe/PowerShell, which
    // have no `unset`, and those shells don't source a profile anyway (the
    // spawned terminal inherits the env we just cleaned).
    eprintln!("[Claude Login] Direct approach failed, trying external terminal");
    // `cfg!` rather than `#[cfg]` so both arms are type-checked on every target.
    let login_cmd = if cfg!(target_os = "windows") {
        "claude login".to_string()
    } else {
        format!("unset {}; claude login", MANAGED_AUTH_VARS.join(" "))
    };
    let result = crate::platform::open_terminal_with_command(&login_cmd);
    match result {
        Ok(()) => Ok(
            "Terminal opened with Claude login. Complete sign-in there, then click Verify below."
                .to_string(),
        ),
        Err(e) => Err(format!(
            "Failed to launch login: {}. Try running 'claude login' manually in PowerShell.",
            e
        )),
    }
}

#[tauri::command]
pub async fn check_auth_status(
    state: tauri::State<'_, ClaudeManager>,
) -> Result<AuthStatus, String> {
    // Check API key first
    let has_api_key = {
        let api_key = state.api_key.lock().map_err(|e| e.to_string())?;
        api_key.is_some()
    };

    if has_api_key {
        return Ok(AuthStatus {
            authenticated: true,
            method: "api_key".to_string(),
        });
    }

    // Check OAuth credentials
    if let Ok(true) = check_oauth_status().await {
        return Ok(AuthStatus {
            authenticated: true,
            method: "oauth".to_string(),
        });
    }

    Ok(AuthStatus {
        authenticated: false,
        method: "none".to_string(),
    })
}

// --- Claude Code Session ---

/// Optional SSH context for running Claude on a remote server
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteContext {
    pub profile_id: String,
    pub remote_path: String,
}

// ── Agent file-deletion guard ───────────────────────────────────────────────
//
// Operon installs a Claude Code PreToolUse hook on the Bash tool that blocks any
// command which would delete or destroy a file or folder. Deletion stays with
// the user: the Explorer's right-click Delete (Operon's own `delete` command,
// which the agent never calls) and commands the user types in the terminal
// themselves. The hook runs even under `--dangerously-skip-permissions` because
// hooks are a separate enforcement layer from the permission system. The agent's
// only deletion vector is the Bash tool (Write/Edit/NotebookEdit cannot remove a
// file), so one hook on Bash covers the whole surface.

/// PreToolUse hook script. Reads the tool-call JSON on stdin, extracts the Bash
/// command, and exits 2 (blocking the call) on any deletion pattern. Portable
/// across macOS and Linux; no `jq` dependency (python3 → node → raw fallback).
const GUARD_HOOK_SCRIPT: &str = r##"#!/usr/bin/env bash
# Operon agent file-deletion guard — Claude Code PreToolUse hook (Bash tool).
# Exit 2 blocks deletion/destruction commands; exit 0 allows everything else.
input=$(cat)
cmd=""
if command -v python3 >/dev/null 2>&1; then
  cmd=$(printf '%s' "$input" | python3 -c 'import sys,json
try:
    d=json.load(sys.stdin)
    print((d.get("tool_input") or {}).get("command","") or "")
except Exception:
    pass' 2>/dev/null)
fi
if [ -z "$cmd" ] && command -v node >/dev/null 2>&1; then
  cmd=$(printf '%s' "$input" | node -e 'let s="";process.stdin.on("data",function(c){s+=c}).on("end",function(){try{process.stdout.write(((JSON.parse(s).tool_input)||{}).command||"")}catch(e){}})' 2>/dev/null)
fi
[ -z "$cmd" ] && cmd="$input"
deny() {
  echo "BLOCKED by Operon: the agent may not delete files or folders. This is a hard safety policy set by the user and cannot be bypassed. Deletion is reserved for the user, who will remove files themselves via the Operon file Explorer (right-click -> Delete) or their own terminal. Do NOT retry this command and do NOT attempt a workaround. Instead, tell the user the exact path(s) you wanted to remove and why, then continue with the rest of the task." >&2
  exit 2
}
# Direct deletion commands. The boundary class includes ' and " so an rm
# laundered through os.system("rm .."), sh -c 'rm ..', eval "rm .." is caught.
printf '%s' "$cmd" | grep -Eq '(^|[;&|(`"'\'']|&&|\|\||[[:space:]])(rm|rmdir|unlink|shred|srm|trash|trash-put)([[:space:]]|$)' && deny
printf '%s' "$cmd" | grep -Eq 'find[[:space:]].*(-delete|-exec[[:space:]]+rm)' && deny
printf '%s' "$cmd" | grep -Eq 'git[[:space:]]+(clean|rm)([[:space:]]|$)' && deny
printf '%s' "$cmd" | grep -Eq 'rsync[[:space:]].*--delete' && deny
printf '%s' "$cmd" | grep -Eq 'mv[[:space:]].+[[:space:]]/dev/null([[:space:]]|$)' && deny
# Deletion via language-interpreter one-liners (python / node / ruby / perl / php).
printf '%s' "$cmd" | grep -Eq 'os\.(remove|unlink|rmdir|removedirs)|shutil\.rmtree|rmtree[[:space:]]*\(|remove_tree|unlinkSync|rmSync|rmdirSync|\.unlink[[:space:]]*\(|\.rmdir[[:space:]]*\(' && deny
printf '%s' "$cmd" | grep -Eq 'File\.(delete|unlink)|FileUtils\.(rm|remove)|Dir\.(delete|rmdir|unlink)|fs\.(rm|unlink|rmdir)|(^|[^a-zA-Z0-9_])unlink[[:space:]]*[("$@]' && deny

# --- Operon pre-submit sbatch review (fail-open: a reviewer error NEVER blocks
# a real job — every branch below degrades to `exit 0`). Reviews the script the
# agent is about to `sbatch` on a DIFFERENT, cheaper model; blocks once with the
# finding so the agent fixes it, then honours a resubmit of the identical script. ---
[ -f "$HOME/.operon/guard/operon-review.env" ] && . "$HOME/.operon/guard/operon-review.env" 2>/dev/null
if [ "${OPERON_REVIEW_ENABLED:-0}" = "1" ] && \
   printf '%s' "$cmd" | grep -Eq '(^|[;&|`"'\''(]|&&|\|\||[[:space:]])sbatch([[:space:]]|$)'; then
  # Runtime toggle: the chat-box "Review sbatch" checkbox drops this marker to
  # skip review for a run without touching the persisted setting.
  [ -f "$HOME/.operon/guard/review-off" ] && exit 0
  _rq="$HOME/.operon/guard/.review-seen"; mkdir -p "$_rq" 2>/dev/null
  # Resolve the script sbatch will actually run.
  #
  # The old rule — "first token in the whole command that is an existing file" —
  # reviewed the WRONG FILE constantly: `source conda.sh && sbatch job.sh` sent
  # conda.sh, `sbatch -o logs/out.txt job.sh` sent the log file, and
  # `sbatch --dependency=.. .job_ids job.sh` sent the job-id list. Whatever it
  # picked came back "no issues", which read as a clean review of the job.
  #
  # Three rules fix that:
  #   1. Only look at tokens AFTER the `sbatch` keyword (kills the conda.sh case).
  #   2. Require a `#!` shebang or a `#SBATCH` directive — sbatch itself requires
  #      the script to start with `#!`, so this is a spec rule, not a guess. It
  #      rejects logs, job-id lists and data files handed to flags.
  #   3. Resolve relative paths against the directory the submit runs in: a
  #      leading `cd DIR &&` or sbatch's own -D/--chdir. Without this the single
  #      most common idiom, `cd /work && sbatch job.sh`, found no file at all and
  #      was recorded as "unavailable" — a silent skip.
  _wd=""; _sf=""; _cands=""; _prev=""; _seen=0
  for _t in $cmd; do
    if [ "$_seen" = "0" ]; then
      [ "$_prev" = "cd" ] && _wd="$_t"
      [ "$_t" = "sbatch" ] && _seen=1
    else
      case "$_t" in
        --chdir=*) _wd=${_t#--chdir=} ;;
        -D?*)      _wd=${_t#-D} ;;
        -*)        [ "$_prev" = "-D" ] || [ "$_prev" = "--chdir" ] && _wd="$_t" ;;
        *)
          if [ "$_prev" = "-D" ] || [ "$_prev" = "--chdir" ]; then _wd="$_t"
          else _cands="$_cands $_t"; fi
          ;;
      esac
    fi
    _prev="$_t"
  done
  # Tokens carry the separator that followed them (`cd /work; sbatch ...` yields
  # `/work;`) and any quoting the user typed. Strip both, or the directory never
  # resolves and the review is silently skipped.
  _unq() { _v=$1; _v=${_v%;}; _v=${_v%&}; _v=${_v#\"}; _v=${_v%\"}; _v=${_v#\'}; _v=${_v%\'}; printf '%s' "$_v"; }
  _wd=$(_unq "$_wd")
  for _c in $_cands; do
    _c=$(_unq "$_c")
    for _p in "${_wd:+$_wd/}$_c" "$_c"; do
      [ -f "$_p" ] || continue
      if [ "$(head -c 2 "$_p" 2>/dev/null)" = '#!' ] || head -n 40 "$_p" 2>/dev/null | grep -q '^#SBATCH'; then
        _sf="$_p"; break
      fi
    done
    [ -n "$_sf" ] && break
  done
  _body=""
  if [ -n "$_sf" ]; then
    _body=$(head -c 160000 "$_sf" 2>/dev/null)
  elif printf '%s' "$cmd" | grep -q -- '--wrap'; then
    _body=$(printf '%s' "$cmd" | sed -E 's/.*--wrap[[:space:]]*=?[[:space:]]*//' | sed -E "s/^['\"]//;s/['\"]$//" | head -c 160000)
  fi
  _rc="${OPERON_REVIEW_CLAUDE:-}"
  [ -z "$_rc" ] && command -v claude >/dev/null 2>&1 && _rc="claude"
  [ -z "$_rc" ] && [ -x "$HOME/.local/bin/claude" ] && _rc="$HOME/.local/bin/claude"
  [ -z "$_rc" ] && command -v npx >/dev/null 2>&1 && _rc="npx --yes @anthropic-ai/claude-code"
  # If we CAN'T review (no reviewer binary, no python3, or no script body), record
  # an 'unavailable' event so the skipped review shows as a grey chip instead of
  # being mistaken for a clean pass — then allow the submit. Shell-only emit so it
  # works even when python3 is the thing that's missing.
  if [ -z "$_rc" ] || [ -z "$_body" ] || ! command -v python3 >/dev/null 2>&1; then
    _ev="${OPERON_REVIEW_EVENTS:-}"
    if [ -n "$_ev" ]; then
      mkdir -p "$(dirname "$_ev")" 2>/dev/null
      _sb=$(basename "${_sf:-inline}" | tr -d '"')
      printf '{"ts":%s,"kind":"sbatch","script":"%s","outcome":"unavailable","n":0,"findings":[]}\n' "$(date +%s 2>/dev/null || echo 0)" "$_sb" >> "$_ev" 2>/dev/null
    fi
    exit 0
  fi
  if [ -n "$_body" ] && [ -n "$_rc" ] && command -v python3 >/dev/null 2>&1; then
    _hh=$(printf '%s' "$_body" | { shasum 2>/dev/null || md5sum 2>/dev/null; } | cut -d' ' -f1)
    if [ -n "$_hh" ] && [ -f "$_rq/$_hh" ]; then exit 0; fi
    _numbered=$(printf '%s' "$_body" | awk '{printf "%4d | %s\n", NR, $0}')
    _pr="You are a skeptical HPC methods reviewer. Review this sbatch script for mistakes that will WASTE HOURS OF CLUSTER COMPUTE or silently produce WRONG RESULTS. Return ONLY a JSON object, no prose: {\"findings\":[{\"severity\":\"blocking\",\"line\":0,\"title\":\"\",\"why_wrong\":\"\",\"fix\":\"\"}]} (severity blocking or high). Report ONLY: outputs/checkpoints written to node-local /tmp or \$TMPDIR (invisible from the login node, destroyed at job end); GPU/deep-learning code with no --gres=gpu; no conda activate / module load before python/R; missing set -euo pipefail; --mem or --time obviously implausible for the work; an array job whose tasks all overwrite one output file. NEVER report style, naming, or formatting. If nothing qualifies, return {\"findings\":[]}. Strongly prefer false negatives.

SCRIPT (${_sf:-inline}):
$_numbered

Return only the JSON object."
    _to=""; command -v timeout >/dev/null 2>&1 && _to="timeout 120"; [ -z "$_to" ] && command -v gtimeout >/dev/null 2>&1 && _to="gtimeout 120"
    # Honour the configured effort, but never let it cost us the review: models
    # that don't support the level make the CLI exit non-zero with empty stdout,
    # so fall back to a plain call rather than degrading to "unavailable".
    _ef=""; [ -n "${OPERON_REVIEW_EFFORT:-}" ] && _ef="--effort ${OPERON_REVIEW_EFFORT}"
    _inv="$_to $_rc -p --model ${OPERON_REVIEW_MODEL:-claude-sonnet-5} --max-turns 1 --output-format json --disallowedTools Read,Write,Edit,Bash,Glob,Grep,WebFetch,WebSearch,NotebookEdit"
    _res=$(printf '%s\n' "$_pr" | $_inv $_ef 2>/dev/null)
    if [ -z "$_res" ] && [ -n "$_ef" ]; then
      _res=$(printf '%s\n' "$_pr" | $_inv 2>/dev/null)
    fi
    _msg=$(printf '%s' "$_res" | OP_SCRIPT="${_sf:-inline}" OP_EVENTS="${OPERON_REVIEW_EVENTS:-}" python3 -c '
import sys,json,re,os,time
def emit(outcome,n,findings):
    p=os.environ.get("OP_EVENTS","")
    if not p: return
    try:
        d=os.path.dirname(p)
        if d: os.makedirs(d,exist_ok=True)
        with open(p,"a") as f:
            f.write(json.dumps({"ts":int(time.time()),"kind":"sbatch","script":os.path.basename(os.environ.get("OP_SCRIPT","")),"outcome":outcome,"n":n,"findings":findings})+"\n")
    except Exception:
        pass
raw=sys.stdin.read()
try:
    txt=json.loads(raw).get("result","")
except Exception:
    txt=""
m=re.search(r"\{.*\}", txt, re.S) if txt else None
findings=None
if m:
    try:
        findings=json.loads(m.group(0)).get("findings", [])
    except Exception:
        findings=None
if findings is None:
    emit("unavailable",0,[]); sys.exit(0)
sev=lambda x: str(x.get("severity","")).lower()
blk=[x for x in findings if sev(x)=="blocking"][:3]
hi=[x for x in findings if sev(x)=="high"][:3]
# A "high" finding used to be thrown away and the submit recorded as "clean" —
# the reviewer said "no issues" about a script it had just found a serious
# problem in. Surface it as its own outcome instead. It still does NOT block:
# "high" means likely-wrong, and a reviewer that stops real jobs on a maybe is
# one the user switches off. Blocking stays reserved for "blocking".
if not blk:
    if hi:
        emit("warned",len(hi),hi)
    else:
        emit("clean",0,[])
    sys.exit(0)
emit("blocked",len(blk),blk)
print("\n".join("- line %s: %s -- %s  FIX: %s" % (x.get("line"), x.get("title",""), x.get("why_wrong",""), x.get("fix","")) for x in blk))
' 2>/dev/null)
    if [ -n "$_msg" ]; then
      [ -n "$_hh" ] && : > "$_rq/$_hh" 2>/dev/null
      {
        echo "BLOCKED by Operon pre-submit review: this sbatch script has a blocking issue that would waste cluster compute or produce wrong results. FIX the script and resubmit. If you are certain this is a false positive, tell the user why, then resubmit the identical script and it will pass."
        printf '%s\n' "$_msg"
      } >&2
      exit 2
    fi
  fi
fi
exit 0
"##;

/// Short notice prepended to the agent prompt so Claude knows up front not to
/// attempt deletions — saves a wasted, hook-denied turn.
const GUARD_PROMPT_NOTICE: &str = "FILE-SAFETY POLICY: You must not delete files or directories. Operon hard-blocks every deletion command the agent issues (rm, rmdir, unlink, shred, find -delete, git clean, git rm, rsync --delete, and similar) — such commands will fail at the tool level. Do not attempt deletions or workarounds. If a file or folder genuinely needs to be removed, state the exact path and ask the user to delete it themselves via the Explorer or their terminal, then carry on with the rest of the task.\n\n";

/// Build the Claude Code settings JSON that registers the guard hook (local mode).
fn guard_settings_json(hook_path: &str) -> String {
    let esc = hook_path.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"hooks":{{"PreToolUse":[{{"matcher":"Bash","hooks":[{{"type":"command","command":"{}"}}]}}]}}}}"#,
        esc
    )
}

/// Reviewer config the guard hook reads (from `operon-review.env`) to decide
/// whether and how to review an `sbatch` before it runs.
struct ReviewGuardCfg {
    enabled: bool,
    model: String,
    effort: String,
    /// Absolute claude path when we can resolve it (local mode); the hook falls
    /// back to `command -v claude` / npx otherwise (remote / alias setups).
    claude: Option<String>,
    /// Per-session file the hook APPENDS one JSON review record to (clean /
    /// blocked / unavailable) so Operon can render a visible review chip. A
    /// `$HOME`-relative path so it resolves on whatever node the hook runs on
    /// (shared home on HPC → readable from the login node).
    events_path: String,
}

/// `$HOME`-relative review-events path for a session (shell-safe session id).
fn review_events_path(session_id: &str) -> String {
    let s: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("$HOME/.operon/reviews/{}.jsonl", s)
}

/// Contents of `operon-review.env` — sourced by the guard hook. String values
/// are single-quoted + escaped; the events path is double-quoted so `$HOME`
/// expands when the hook sources it.
fn review_env_content(cfg: &ReviewGuardCfg) -> String {
    let q = |s: &str| s.replace('\'', "'\\''");
    let mut out = format!(
        "OPERON_REVIEW_ENABLED={}\nOPERON_REVIEW_MODEL='{}'\nOPERON_REVIEW_EFFORT='{}'\nOPERON_REVIEW_EVENTS=\"{}\"\n",
        if cfg.enabled { "1" } else { "0" },
        q(&cfg.model),
        q(&cfg.effort),
        cfg.events_path,
    );
    if let Some(c) = cfg.claude.as_deref().filter(|c| !c.is_empty()) {
        out.push_str(&format!("OPERON_REVIEW_CLAUDE='{}'\n", q(c)));
    }
    out
}

/// Build the reviewer guard config from settings. Enabled only when BOTH the
/// reviewer and its auto-sbatch trigger are on.
fn review_guard_cfg(
    settings_state: &tauri::State<'_, super::settings::SettingsManager>,
    session_id: &str,
    claude: Option<String>,
) -> ReviewGuardCfg {
    let events_path = review_events_path(session_id);
    match settings_state.settings.lock() {
        Ok(s) => ReviewGuardCfg {
            enabled: s.reviewer_enabled && s.reviewer_auto_sbatch,
            model: s.reviewer_model.clone(),
            effort: s.reviewer_effort.clone(),
            claude,
            events_path,
        },
        Err(_) => ReviewGuardCfg {
            enabled: false,
            model: "claude-sonnet-5".to_string(),
            effort: "low".to_string(),
            claude,
            events_path,
        },
    }
}

/// Shell snippet that writes the guard hook + settings + review env into
/// `$HOME/.operon/guard` on a remote host. Prepended to the remote run script so
/// the files exist before `claude` starts. Idempotent — overwrites each session.
fn remote_guard_setup_block(cfg: &ReviewGuardCfg) -> String {
    let mut s = String::new();
    s.push_str("__og=\"$HOME/.operon/guard\"; mkdir -p \"$__og\" 2>/dev/null\n");
    // Clear a stale runtime toggle so this session honors the persisted setting;
    // the chat-box checkbox re-creates it if the user turns review off mid-run.
    s.push_str("rm -f \"$__og/review-off\" 2>/dev/null\n");
    // Drop the block-exemption cache. A marker is written when a script is
    // blocked so the agent's immediate resubmit of the identical script goes
    // through — that override is meant to last one submit, not forever. Left on
    // disk it silently exempted that script from review in every future session.
    s.push_str("rm -rf \"$__og/.review-seen\" 2>/dev/null\n");
    s.push_str("cat > \"$__og/operon-guard.sh\" <<'__OPERON_GUARD_EOF__'\n");
    s.push_str(GUARD_HOOK_SCRIPT);
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("__OPERON_GUARD_EOF__\n");
    s.push_str("chmod +x \"$__og/operon-guard.sh\" 2>/dev/null\n");
    s.push_str("printf '%s' '{\"hooks\":{\"PreToolUse\":[{\"matcher\":\"Bash\",\"hooks\":[{\"type\":\"command\",\"command\":\"'\"$__og\"'/operon-guard.sh\"}]}]}}' > \"$__og/operon-guard-settings.json\"\n");
    // Reviewer env, written via a heredoc so quoting survives the SSH chain.
    s.push_str("cat > \"$__og/operon-review.env\" <<'__OPERON_REVIEW_EOF__'\n");
    s.push_str(&review_env_content(cfg));
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("__OPERON_REVIEW_EOF__\n");
    s
}

/// Write the guard hook + settings under `~/.operon/guard` and append
/// `--settings <path>` to `claude_cmd`. Unix-only — the hook is a bash script;
/// on Windows the call is a no-op (remote sessions still get the guard).
fn install_local_guard(claude_cmd: &mut String, cfg: &ReviewGuardCfg) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let dir = home.join(".operon").join("guard");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let hook = dir.join("operon-guard.sh");
        let settings = dir.join("operon-guard-settings.json");
        if std::fs::write(&hook, GUARD_HOOK_SCRIPT).is_err() {
            return;
        }
        let _ = std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755));
        if std::fs::write(&settings, guard_settings_json(&hook.to_string_lossy())).is_err() {
            return;
        }
        // Reviewer env for the sbatch-review hook (best-effort; hook fails open).
        let _ = std::fs::write(dir.join("operon-review.env"), review_env_content(cfg));
        // Clear a stale runtime toggle so this session honors the persisted
        // setting; the chat-box checkbox re-creates it if turned off mid-run.
        let _ = std::fs::remove_file(dir.join("review-off"));
        // Same one-session lifetime for the block-exemption cache as the remote
        // path — a blocked script must be re-reviewed in the next session.
        let _ = std::fs::remove_dir_all(dir.join(".review-seen"));
        claude_cmd.push_str(&format!(
            " --settings '{}'",
            settings.to_string_lossy().replace('\'', "'\\''")
        ));
        eprintln!("[operon] local agent guard installed (file-deletion + sbatch review)");
    }
    #[cfg(not(unix))]
    {
        let _ = (claude_cmd, cfg); // guard hook is a bash script — unix-only for now
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_claude_session(
    state: tauri::State<'_, ClaudeManager>,
    terminal_state: tauri::State<'_, super::terminal::TerminalManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    proxy_state: tauri::State<'_, super::proxy::ProxyManager>,
    app: tauri::AppHandle,
    session_id: String,
    prompt: String,
    project_path: String,
    model: Option<String>,
    max_turns: Option<u32>,
    resume_session: Option<String>,
    mode: Option<String>,
    remote: Option<RemoteContext>,
    use_terminal: Option<bool>,
    terminal_id: Option<String>,
) -> Result<(), String> {
    // Get API key
    let api_key = {
        let key = state.api_key.lock().map_err(|e| e.to_string())?;
        key.clone()
    };

    let mode = mode.unwrap_or_else(|| "agent".to_string());
    eprintln!(
        "[operon] start_claude_session: mode='{}', resume={:?}, max_turns={:?}",
        mode, resume_session, max_turns
    );

    // Local custom-provider sessions need the bundled Anthropic↔OpenAI proxy
    // running BEFORE we build the env. Ollama / LM Studio / vLLM speak only
    // OpenAI Chat-Completions; Claude Code speaks Anthropic Messages. The proxy
    // is killed on app close and is otherwise only started by the Settings
    // "Start proxy" button, so without this an Ollama chat would silently fall
    // back to the raw upstream URL (which 404s on /v1/messages) and fail with a
    // confusing "empty response". Start (or reuse) it on demand here, and fail
    // with a clear message if it can't bind. Local only: a 127.0.0.1 proxy is
    // unreachable from a remote compute node (that path needs a reverse tunnel).
    if remote.is_none() {
        let (provider, use_proxy, upstream, up_key) = {
            let s = settings_state.settings.lock().map_err(|e| e.to_string())?;
            (
                s.ai_provider.clone(),
                s.use_translation_proxy,
                s.custom_base_url.trim().to_string(),
                s.custom_api_key.clone(),
            )
        };
        if provider == "custom" && use_proxy && !upstream.is_empty() {
            let key = if up_key.is_empty() {
                None
            } else {
                Some(up_key)
            };
            super::proxy::ensure_proxy(&app, proxy_state.inner(), &upstream, key.as_deref())
                .await
                .map_err(|e| {
                    format!(
                        "Couldn't start the translation proxy required for the custom \
                         endpoint ({}). Open Settings → Custom provider and verify the \
                         Base URL, then try Start proxy.",
                        e
                    )
                })?;
        }
    }

    // --- Check for existing plan files in the target directory ---
    // This gives Claude context about previous planning sessions in this folder.
    let existing_plan = if let Some(ref ctx) = remote {
        // Remote: read implementation_plan.md via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
        };
        if let Some(prof) = profile {
            let check_cmd = format!(
                "cat '{}'/implementation_plan.md 2>/dev/null || echo ''",
                ctx.remote_path.replace('\'', "'\\''")
            );
            super::ssh::ssh_exec(&prof, &check_cmd).unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        // Local: read implementation_plan.md from project path
        let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
        std::fs::read_to_string(&plan_path).unwrap_or_default()
    };
    let existing_plan = existing_plan.trim().to_string();

    // Build permission flag based on settings
    let permission_mode = {
        let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
        settings.permission_mode.clone()
    };
    // Permission levels control how Claude Code handles tool approvals:
    //  full_auto  — skip all permission prompts (fastest, default)
    //  safe_mode  — allow only read-only tools without prompts; Claude will be instructed
    //              to avoid destructive operations and ask the user before modifying files
    //  supervised — no permission skip; Claude runs in standard interactive mode
    //              and prompts for each tool use (works via terminal passthrough)
    let permission_flag = match permission_mode.as_str() {
        "supervised" => "",
        "safe_mode" => "--dangerously-skip-permissions",
        _ => "--dangerously-skip-permissions", // full_auto
    };
    // For safe_mode, we prepend a safety instruction to every prompt
    let safety_prefix = if permission_mode == "safe_mode" {
        "IMPORTANT SAFETY CONSTRAINT: You are in SAFE MODE. You may freely read files, search, \
         and browse, but you MUST ask the user for explicit confirmation before: \
         (1) writing or editing any file, (2) running any bash command that modifies state \
         (installs, deletes, moves, or overwrites), (3) creating new files. \
         For any such action, describe what you plan to do and wait for the user to say 'yes' or 'go ahead' \
         before executing. Read-only commands (cat, ls, grep, find, head, etc.) are always safe to run.\n\n"
            .to_string()
    } else {
        String::new()
    };

    // If there's an existing plan, prepend it as context for agent/ask modes.
    // Cap the inline size: anything over ~20KB gets replaced with a pointer that
    // tells the agent to Read the file as needed. Inlining a huge plan blows up
    // the remote run-script (the prompt becomes a 100KB+ `-p '...'` arg), which
    // forces Operon's chunked SSH upload path and is fragile on HPC clusters with
    // ControlMaster / Duo MFA / slow auth — silent upload failures result.
    const PLAN_INLINE_LIMIT: usize = 75_000;
    let context_prefix = {
        let plan_ctx = if existing_plan.is_empty() || mode == "plan" {
            String::new()
        } else if existing_plan.len() <= PLAN_INLINE_LIMIT {
            format!(
                "CONTEXT: There is an existing implementation_plan.md in this directory from a previous planning session. \
                 Here is its content:\n\n---\n{}\n---\n\n\
                 Use this plan as context for your work. If the user's request relates to this plan, follow it. \
                 If the request is unrelated, you can ignore the plan.\n\n",
                existing_plan
            )
        } else {
            eprintln!(
                "[operon] plan too large to inline ({} bytes > {} cap) — passing as a Read-it-yourself pointer",
                existing_plan.len(),
                PLAN_INLINE_LIMIT
            );
            format!(
                "CONTEXT: There is an existing implementation_plan.md in this directory from a previous planning session ({} KB — too large to inline). \
                 Use the Read tool to load it if the user's request relates to the plan, then follow it. \
                 If the request is unrelated, you can ignore the plan.\n\n",
                existing_plan.len() / 1024
            )
        };
        // Only tell the agent deletions are hard-blocked when the guard is
        // actually installed: remote sessions (remote_guard_setup_block always
        // runs on the Linux host) or local Mac/Linux (install_local_guard is
        // #[cfg(unix)]). On local WINDOWS install_local_guard is a no-op, so the
        // hook doesn't exist — claiming deletions are blocked there would be a
        // lie that invites real data loss (the agent runs `rm -rf` believing it
        // will be caught). Drop the notice in that one case.
        let guard_notice = if remote.is_some() || cfg!(unix) {
            GUARD_PROMPT_NOTICE
        } else {
            ""
        };
        format!("{}{}{}", guard_notice, safety_prefix, plan_ctx)
    };

    // Generate a human-readable timestamp for plan sections
    let now_timestamp = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Format as YYYY-MM-DD HH:MM (UTC)
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        // Compute year/month/day from epoch days
        let mut y = 1970i64;
        let mut remaining = days as i64;
        loop {
            let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                366
            } else {
                365
            };
            if remaining < days_in_year {
                break;
            }
            remaining -= days_in_year;
            y += 1;
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let month_days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut m = 0usize;
        for &md in &month_days {
            if remaining < md as i64 {
                break;
            }
            remaining -= md as i64;
            m += 1;
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} UTC",
            y,
            m + 1,
            remaining + 1,
            hours,
            minutes
        )
    };
    // Also compute a filename-safe version for archiving
    let now_filename = now_timestamp.replace(' ', "_").replace(':', "");

    // --- Plan mode: archive existing plan before writing a new one ---
    // This keeps implementation_plan.md clean (always ONE active plan) while
    // preserving full history in .operon/plan_history/ for reference.
    if mode == "plan" && !existing_plan.is_empty() {
        if let Some(ref ctx) = remote {
            // Remote: archive via SSH
            let profile = {
                let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
            };
            if let Some(prof) = profile {
                let archive_cmd = format!(
                    "mkdir -p '{base}/.operon/plan_history' && \
                     cp '{base}/implementation_plan.md' '{base}/.operon/plan_history/plan_{ts}.md' 2>/dev/null || true",
                    base = ctx.remote_path.replace('\'', "'\\''"),
                    ts = now_filename
                );
                let _ = super::ssh::ssh_exec(&prof, &archive_cmd);
            }
        } else {
            // Local: archive to .operon/plan_history/
            let history_dir = std::path::Path::new(&project_path)
                .join(".operon")
                .join("plan_history");
            let _ = std::fs::create_dir_all(&history_dir);
            let archive_name = format!("plan_{}.md", now_filename);
            let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
            let _ = std::fs::copy(&plan_path, history_dir.join(&archive_name));
        }
    }

    let mut claude_cmd = match mode.as_str() {
        "plan" => {
            // Plan mode: write a FRESH implementation_plan.md
            // The previous plan (if any) was just archived to .operon/plan_history/
            // Give Claude the old plan as read-only context so it can build on it,
            // but instruct it to write a completely new file.
            let existing_plan_context = if !existing_plan.is_empty() {
                format!(
                    "\n\nCONTEXT: The previous implementation plan (now archived) is shown below for reference. \
                     Use it to understand what has already been planned or completed. \
                     You may reference, build upon, or supersede it — but write your plan as a \
                     fresh, self-contained document.\n\n\
                     <previous_plan>\n{}\n</previous_plan>",
                    existing_plan
                )
            } else {
                String::new()
            };

            let plan_prompt = format!(
                "{}You are in PLAN mode.\n\n\
                 CRITICAL INSTRUCTION: Your ONLY action is to write a file called 'implementation_plan.md'. \
                 Do NOT run bash commands. Do NOT read files. Do NOT search for anything. Do NOT check MCP configurations. \
                 Do NOT use any tools except the Write tool to create implementation_plan.md. \
                 You already have all the context you need in this prompt.\n\n\
                 Write the plan to 'implementation_plan.md' in the current directory. \
                 This should be a FRESH, self-contained plan.\
                 \n\nFORMATTING RULES:\
                 \n- Start with: # Implementation Plan: <short title>\
                 \n- Add: **Date:** {}\
                 \n- Then include: 1) Overview of the task, 2) Step-by-step implementation steps, \
                 3) Files to create or modify, 4) Dependencies needed, 5) Testing strategy, \
                 6) Potential risks or considerations.\
                 \n- Include a '## Status' section with each step marked as [ ] (pending) \
                 so that Agent mode can track progress.\
                 \n- If the previous plan had steps marked [x] (completed), you may note those as \
                 already done in your new plan so Agent mode knows not to redo them.{}\
                 \n\nREMEMBER: Do NOT run any bash/shell commands. Just write the plan file directly.\
                 \n\nThe user's request: {}",
                safety_prefix,
                now_timestamp,
                existing_plan_context,
                prompt
            );
            format!(
                "claude {} -p '{}' --verbose --output-format stream-json",
                permission_flag,
                plan_prompt.replace('\'', "'\\''")
            )
        }
        "report" => {
            // Report mode: Claude drafts a scientific report based on project files.
            // The frontend sends a structured prompt with inline file contents, methods info,
            // PubMed citations, and user instructions.
            //
            // IMPORTANT: The prompt can be 200KB+ (31 files × 8KB each). We CANNOT pass
            // this via -p '...' because shell argument escaping breaks on file contents
            // (single quotes, backticks, $variables, heredoc delimiters in CSV/code data).
            // Instead, write the prompt to a temp file and pipe it to Claude via stdin.
            let tool_instruction =
                "CRITICAL: All file contents are already provided inline in this prompt inside <file> tags. \
                 Do NOT use any tools — no Read, no Bash, no Glob, no Grep, no file operations whatsoever. \
                 You have exactly 1 turn. Write the entire report directly from the provided file contents and context. \
                 Any attempt to use tools will fail and waste your only turn.";
            let report_prompt = format!(
                "You are in REPORT mode — a scientific report generator for bioinformatics analyses. \
                 Your task is to produce a professional analysis report based on the project files and context provided.\n\n\
                 {}\n\n\
                 RULES:\n\
                 1. Write in formal scientific prose suitable for a research report.\n\
                 2. Every factual claim about biology must cite a PubMed reference using [N] notation.\n\
                 3. The Methods section must list tools with version numbers — omit infrastructure details (SLURM, conda envs, HPC configs).\n\
                 4. Interpret results biologically — don't just describe what the plots show, explain what they mean.\n\
                 5. The Discussion should connect findings to the broader literature.\n\
                 6. Use the implementation_plan.md (if available) to understand what analyses were performed.\n\n\
                 Output the report NOW as structured markdown sections (# Title, ## Abstract, ## Introduction, \
                 ## Results, ## Discussion, ## Methods, ## References). \
                 Write each section thoroughly — this will become a PDF.\n\n\
                 {}{}",
                tool_instruction,
                context_prefix,
                // Use the raw prompt here — no shell escaping needed since it goes to a file
                prompt
            );

            // Write prompt to a local temp file — this bypasses all shell escaping issues
            let prompt_file =
                std::env::temp_dir().join(format!("operon-report-prompt-{}.txt", session_id));
            let prompt_file_str = prompt_file.to_string_lossy().to_string();
            std::fs::write(&prompt_file, &report_prompt)
                .map_err(|e| format!("Failed to write report prompt file: {}", e))?;
            eprintln!(
                "[operon] Report prompt written to {} ({} bytes)",
                prompt_file_str,
                report_prompt.len()
            );

            // Pipe the prompt from a file via stdin (`-p` = print/non-interactive
            // mode). This runs through a POSIX shell on every platform (Git Bash
            // on Windows), so `cat` plus a single-quoted, forward-slash path
            // works identically — `type` is a cmd.exe builtin and would be wrong
            // here even on Windows.
            let prompt_file_posix =
                crate::platform::common::normalize_display_path(&prompt_file_str);
            let pipe_cmd = format!(
                "cat '{}' | claude {} -p --verbose --output-format stream-json",
                prompt_file_posix.replace('\'', "'\\''"),
                permission_flag
            );
            pipe_cmd
        }
        "ask" => {
            // Ask mode: no tool use, answer questions with scientific rigor
            let ask_prompt = format!(
                "You are in ASK mode — a scientific Q&A assistant for bioinformatics researchers. \
                 Do NOT use any tools (no file reads, writes, or bash commands). \
                 Answer the user's question using your knowledge and any PubMed literature provided in the prompt. \
                 If PubMed articles are included in <pubmed_literature> tags, you MUST:\n\
                 1. Directly reference and cite the provided articles by number [1], [2], etc.\n\
                 2. Include PubMed URLs so the user can access the original papers.\n\
                 3. Base your answer primarily on the evidence in these articles.\n\
                 4. End your response with a formatted References section.\n\
                 If you need to look at files or run commands, tell the user to switch to Agent mode.\n\n{}\
                 {}",
                context_prefix,
                prompt
            );
            format!(
                "claude {} -p '{}' --verbose --output-format stream-json --max-turns 1",
                permission_flag,
                ask_prompt.replace('\'', "'\\''")
            )
        }
        _ => {
            // Agent mode (default): full tool use
            // If there's a plan, tell Claude to follow it and update status
            let agent_prompt = if !existing_plan.is_empty() {
                format!(
                    "{}IMPORTANT: As you complete steps from the implementation plan, \
                     update implementation_plan.md to mark completed steps with [x] \
                     so progress is tracked.\n\n{}",
                    context_prefix, prompt
                )
            } else {
                format!("{}{}", context_prefix, prompt)
            };
            format!(
                "claude {} -p '{}' --verbose --output-format stream-json",
                permission_flag,
                agent_prompt.replace('\'', "'\\''")
            )
        }
    };

    if let Some(m) = &model {
        // Single-quote: the model string can come from a free-text custom-model
        // field, so an apostrophe/space would otherwise corrupt the shell command.
        claude_cmd.push_str(&format!(" --model '{}'", m.replace('\'', "'\\''")));

        // Effort level (Opus 5 / Opus 4.8 / Sonnet 5). Only append when the model
        // actually supports the chosen level — otherwise silently skip so a
        // user with "max" pinned can switch to a model that doesn't have it
        // (e.g. Haiku 4.5) without the CLI rejecting the flag.
        let effort = {
            let s = settings_state.settings.lock().map_err(|e| e.to_string())?;
            s.effort.clone()
        };
        if !effort.is_empty() && super::models::model_supports_effort_level(m, &effort) {
            claude_cmd.push_str(&format!(" --effort {}", effort));
        }
    }
    if mode == "plan" {
        claude_cmd.push_str(" --max-turns 3");
    } else if mode == "report" {
        // Report mode: all file contents are pre-read and injected into the prompt.
        // 1 turn is all that's needed — block all tools to prevent wasted reads.
        let report_turns = max_turns.unwrap_or(1);
        claude_cmd.push_str(&format!(" --max-turns {}", report_turns));
        claude_cmd.push_str(" --disallowedTools Read,Bash,Glob,Grep");
    } else if let Some(turns) = max_turns {
        claude_cmd.push_str(&format!(" --max-turns {}", turns));
    } else {
        // Default max-turns for agent mode to prevent infinite loops.
        // 30 turns is enough for complex multi-step tasks while ensuring
        // the agent eventually stops if it gets stuck in a polling cycle.
        claude_cmd.push_str(" --max-turns 30");
    }
    if let Some(resume) = &resume_session {
        claude_cmd.push_str(&format!(" --resume '{}'", resume.replace('\'', "'\\''")));
    }

    // Persistent-job-watcher hand-off rule. Tells the agent to register any
    // long-running SLURM job with Operon and end the turn, rather than poll
    // it inside the conversation (which dies when Operon closes and costs
    // tokens while alive). Operon will surface the completion as a banner
    // and the user can resume on demand.
    let slurm_rule = "PERSISTENT JOB WATCHER (Operon):\\n\
        After submitting a SLURM job that you expect to take more than 10 minutes (any sbatch with --time > 00:10:00, or anything you cannot reasonably wait on inline), DO NOT enter a polling loop. Operon runs a remote watchdog that survives the app being closed and will notify the user when the job completes.\\n\
        \\n\
        Instead:\\n\
        1. Capture the job id from the sbatch output (e.g. \\\"Submitted batch job 52805324\\\").\\n\
        2. Tell the user: 'Submitted job <id>. Watcher will notify you when it completes — feel free to close Operon.'\\n\
        3. State what file or condition you expect the job to produce (e.g. 'expected output: config/array_index.tsv').\\n\
        4. END YOUR TURN. Do not loop on squeue/sacct. Do not sleep. Do not poll.\\n\
        \\n\
        The user will resume the conversation with the job result when ready.";
    // POSIX-escape the rule before wrapping in outer single quotes. The rule
    // contains inner single quotes ("Submitted job <id>. ...") and angle
    // brackets — without this, the inner ' closes the outer quote and bash
    // interprets `<id>` as a stdin redirection from a file called `id`,
    // producing the misleading "bash: line 1: id: No such file or directory"
    // (which has nothing to do with the /usr/bin/id coreutil).
    claude_cmd.push_str(&format!(
        " --append-system-prompt '{}'",
        slurm_rule.replace('\'', "'\\''")
    ));

    eprintln!(
        "[operon] Final claude command (first 200 chars): {}",
        &claude_cmd[..claude_cmd.len().min(200)]
    );

    // Sync MCP servers into Claude Code's native config so they're available
    // without relying on --mcp-config (which has known bugs in some Claude Code versions).
    let mcp_servers = {
        let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
        settings.mcp_servers.clone()
    };
    let _ = super::mcp::sync_mcp_servers_to_claude(&mcp_servers);

    // Also generate mcp-config.json and pass --mcp-config as fallback
    // (needed for remote/HPC sessions where Claude runs on a different host).
    if let Some(config_path) = super::mcp::generate_mcp_config(&mcp_servers)? {
        // Shell-escape the path in case it contains spaces
        claude_cmd.push_str(&format!(
            " --mcp-config '{}'",
            config_path.replace('\'', "'\\''")
        ));
    }

    let shell = claude_shell()?;

    let use_terminal = use_terminal.unwrap_or(false);

    // --- Persist session metadata so it survives app restarts ---
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Derive session name from first ~50 chars of prompt
    let session_name = {
        let trimmed = prompt.trim();
        if trimmed.len() > 50 {
            format!(
                "{}...",
                &trimmed[..trimmed
                    .char_indices()
                    .nth(50)
                    .map(|(i, _)| i)
                    .unwrap_or(trimmed.len())]
            )
        } else {
            trimmed.to_string()
        }
    };

    let meta = SessionMetadata {
        session_id: session_id.clone(),
        claude_session_id: resume_session.clone(),
        project_path: project_path.clone(),
        profile_id: remote.as_ref().map(|r| r.profile_id.clone()),
        remote_path: remote.as_ref().map(|r| r.remote_path.clone()),
        mode: mode.clone(),
        model: model.clone(),
        created_at: now,
        last_activity: now,
        status: "running".to_string(),
        use_terminal,
        terminal_id: terminal_id.clone(),
        name: Some(session_name),
    };
    let _ = save_session_to_disk(&meta);

    // --- Serialize Operon's own concurrent OAuth-refresh windows (remote only) ---
    // A starting `claude` whose access token has expired refreshes + rotates the
    // shared OAuth token. Two of Operon's launches doing that at the same instant
    // race on the NFS-backed credentials file, and the loser's failed refresh can
    // permanently wipe the refresh token. Hold a per-host lock across claude's
    // brief startup-refresh window so launches serialize. It is uncontended in the
    // common case (sequential turns take far longer than the window); only a
    // genuinely concurrent second launch waits. A detached task releases it so it
    // never blocks the long-running stream. (The auth check no longer refreshes —
    // it is a pure file read — so it is intentionally NOT gated here.)
    let refresh_lock = remote
        .as_ref()
        .map(|ctx| state.refresh_lock_for(&ctx.profile_id));
    if let Some(lock) = refresh_lock {
        let guard = lock.lock_owned().await;
        tokio::spawn(async move {
            // Generous enough to cover a slow HPC login-node token refresh.
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            drop(guard);
        });
    }

    // --- TERMINAL MODE: run Claude inside the user's existing terminal session ---
    // This reuses their tmux/compute node/conda environment
    if use_terminal {
        if let (Some(ref ctx), Some(ref tid)) = (&remote, &terminal_id) {
            let profile = {
                let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                profiles
                    .iter()
                    .find(|p| p.id == ctx.profile_id)
                    .cloned()
                    .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
            };

            // For HPC terminal mode, write MCP config to the remote shared filesystem
            // so the claude process on the compute node can access it.
            if let Some(mcp_json) = super::mcp::generate_mcp_config_json(&mcp_servers)? {
                let mcp_config_remote = format!("{}/.operon-mcp-config.json", ctx.remote_path);
                let encoded_json =
                    base64::engine::general_purpose::STANDARD.encode(mcp_json.as_bytes());
                let write_cmd = format!(
                    "echo '{}' | base64 -d > '{}'",
                    encoded_json,
                    mcp_config_remote.replace('\'', "'\\''")
                );
                let _ = super::ssh::ssh_exec(&profile, &write_cmd);
                // Replace the local config path in claude_cmd with the remote path
                if let Some(local_path) = super::mcp::generate_mcp_config(&mcp_servers)? {
                    claude_cmd = claude_cmd.replace(
                        &format!("--mcp-config '{}'", local_path),
                        &format!(
                            "--mcp-config '{}'",
                            mcp_config_remote.replace('\'', "'\\''")
                        ),
                    );
                }
            }

            // For report mode, upload the local prompt file to the remote shared filesystem
            // so the `cat prompt | claude` command works on the compute node.
            // Uses SCP (with ControlMaster reuse) — reliable for any file size, no encoding issues.
            if mode == "report" {
                let local_prompt_file = std::env::temp_dir()
                    .join(format!("operon-report-prompt-{}.txt", session_id))
                    .to_string_lossy()
                    .to_string();
                let remote_prompt_file = format!(
                    "{}/.operon-report-prompt-{}.txt",
                    ctx.remote_path, session_id
                );
                if std::path::Path::new(&local_prompt_file).exists() {
                    let host_str = format!("{}@{}", profile.user, profile.host);
                    let mut scp_args: Vec<String> = vec![
                        "-o".to_string(),
                        "BatchMode=yes".to_string(),
                        "-o".to_string(),
                        "ConnectTimeout=10".to_string(),
                    ];
                    // Reuse ControlMaster socket only if the master is actually live.
                    if let Some(sock) = super::ssh::live_control_socket(&profile) {
                        scp_args.push("-o".to_string());
                        scp_args.push(format!("ControlPath={}", sock.to_string_lossy()));
                    }
                    if profile.port != 22 {
                        scp_args.push("-P".to_string());
                        scp_args.push(profile.port.to_string());
                    }
                    if let Some(key) = &profile.key_file {
                        if std::path::Path::new(key).exists() {
                            scp_args.push("-i".to_string());
                            scp_args.push(key.clone());
                        }
                    }
                    scp_args.push(local_prompt_file.clone());
                    scp_args.push(format!("{}:{}", host_str, remote_prompt_file));

                    let scp_result =
                        hide_window(std::process::Command::new("scp").args(&scp_args)).output();
                    match scp_result {
                        Ok(output) if output.status.success() => {
                            let file_size = std::fs::metadata(&local_prompt_file)
                                .map(|m| m.len())
                                .unwrap_or(0);
                            eprintln!(
                                "[operon] SCP uploaded report prompt to remote: {} ({} bytes)",
                                remote_prompt_file, file_size
                            );
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            eprintln!("[operon] SCP upload failed: {}", stderr);
                        }
                        Err(e) => {
                            eprintln!("[operon] SCP command failed: {}", e);
                        }
                    }
                    // Replace the local path in claude_cmd with the remote path
                    claude_cmd = claude_cmd.replace(&local_prompt_file, &remote_prompt_file);
                }
            }

            // Create a unique output file path on the SHARED filesystem (not /tmp which is node-local).
            // On HPC systems, /tmp is local to each node — the compute node writes the file but
            // the tail SSH connects to the login node, which can't see compute-node /tmp.
            // Use the remote working directory which is on a shared NFS/GPFS filesystem.
            let output_file = format!("{}/.operon-{}.jsonl", ctx.remote_path, session_id);
            let done_file = format!("{}/.operon-{}.done", ctx.remote_path, session_id);

            // Write the claude command to a temp script, then `source` it.
            // This keeps the terminal clean (only "source /path/.cf-run.sh" is visible)
            // while preserving the user's shell aliases (unlike piping to `bash`).
            let script_file = format!("{}/.operon-run-{}.sh", ctx.remote_path, session_id);
            // Clean up the report prompt file after Claude finishes (if it exists)
            let prompt_cleanup = if mode == "report" {
                format!(
                    "; rm -f '{}/.operon-report-prompt-{}.txt'",
                    ctx.remote_path.replace('\'', "'\\''"),
                    session_id
                )
            } else {
                String::new()
            };
            // Export provider env vars in the script so Claude can authenticate
            // on the remote. In terminal mode the user's shell may not have
            // them set. For custom endpoints this emits ANTHROPIC_BASE_URL +
            // ANTHROPIC_AUTH_TOKEN instead of ANTHROPIC_API_KEY.
            let provider_env = ai_provider_env(&settings_state, &proxy_state, &api_key);
            let provider_unset = ai_provider_env_unset(&provider_env);
            // NOTE: this script is `source`d into the user's live tmux shell (see
            // the write below) rather than run in a subshell, because that is the
            // only way `claude` still resolves through their shell alias on HPC.
            // The `unset` therefore persists in that shell for the rest of the
            // session — deliberate: a stale credential there would shadow the
            // subscription for anything else the user runs too.
            let api_key_line = ai_provider_env_exports(&provider_env, &provider_unset);
            // Pre-flight warmup: the Anthropic launcher (~/.local/bin/claude etc.) checks
            // its npm-cached package on every invocation and shows an interactive
            //   "Need to install @anthropic-ai/claude-code@X.Y.Z — Ok to proceed? (y)"
            // prompt the first time after install/update. Because Operon redirects the
            // real run's stdout/stderr to a file, that prompt deadlocks waiting for
            // input. Running the launcher once with `yes y` piped in auto-confirms the
            // install and primes the cache; subsequent runs return instantly. The
            // warmup is best-effort (`|| true`) — if claude isn't installed at all, the
            // real command will fail in the same way it would have without the warmup.
            // Install the agent file-deletion guard on the remote ($HOME/.operon/guard)
            // and point this run's claude at it via --settings.
            let guard_block =
                remote_guard_setup_block(&review_guard_cfg(&settings_state, &session_id, None));
            claude_cmd.push_str(" --settings \"$HOME/.operon/guard/operon-guard-settings.json\"");
            // The run script is the ONLY writer of `.done` — see the tail script,
            // which is a read-only observer. It therefore also owns clearing a
            // stale one: a `.done` left behind by a killed tail (or by a previous
            // turn that was interrupted) made the next turn's tail see "already
            // finished" and exit immediately, so the new agent streamed nothing.
            // Clearing both files here means every run starts from a clean slate.
            let script_content = format!(
                "{}{}{}cd '{}' && rm -f '{}' '{}'; (yes y 2>/dev/null | claude --version >/dev/null 2>&1 || true); {} > '{}' 2>&1; echo $? > '{}'{}",
                REMOTE_PATH_PREFIX,
                api_key_line,
                guard_block,
                ctx.remote_path.replace('\'', "'\\''"),
                output_file.replace('\'', "'\\''"),
                done_file.replace('\'', "'\\''"),
                claude_cmd,
                output_file.replace('\'', "'\\''"),
                done_file.replace('\'', "'\\''"),
                prompt_cleanup,
            );

            // Upload the script to the remote via ssh_exec + base64.
            // Uses chunked transfer to avoid ControlMaster socket message size
            // limits (~256KB). Each chunk is appended to a temp b64 file on the
            // remote, then decoded in one shot.
            {
                let b64_script =
                    base64::engine::general_purpose::STANDARD.encode(script_content.as_bytes());
                let escaped_script = script_file.replace('"', "\\\"");
                let tmp_b64 = format!("{}.__b64__", escaped_script);
                const CHUNK_SIZE: usize = 140_000;
                eprintln!(
                    "[operon] script upload starting: target={} script_bytes={} b64_bytes={} chunked={}",
                    script_file,
                    script_content.len(),
                    b64_script.len(),
                    b64_script.len() > CHUNK_SIZE
                );

                if b64_script.len() <= CHUNK_SIZE {
                    // Small script — single command
                    let write_cmd = format!(
                        "printf %s {} | base64 -d > \"{}\"",
                        b64_script, escaped_script,
                    );
                    let t0 = std::time::Instant::now();
                    crate::commands::ssh::ssh_exec(&profile, &write_cmd).map_err(|e| {
                        eprintln!("[operon] script upload FAILED (single-shot): {}", e);
                        format!("Failed to create run script on remote: {}", e)
                    })?;
                    eprintln!(
                        "[operon] script upload OK (single-shot) in {:?}",
                        t0.elapsed()
                    );
                } else {
                    // Large script — write base64 in chunks, then decode
                    let mut offset = 0;
                    let mut first = true;
                    let t0 = std::time::Instant::now();
                    let mut chunk_idx = 0;
                    while offset < b64_script.len() {
                        let end = std::cmp::min(offset + CHUNK_SIZE, b64_script.len());
                        let chunk = &b64_script[offset..end];
                        let redirect = if first { ">" } else { ">>" };
                        let cmd = format!("printf %s {} {} \"{}\"", chunk, redirect, tmp_b64,);
                        crate::commands::ssh::ssh_exec(&profile, &cmd).map_err(|e| {
                            eprintln!("[operon] script upload chunk {} FAILED: {}", chunk_idx, e);
                            format!("Failed to upload script chunk to remote: {}", e)
                        })?;
                        first = false;
                        offset = end;
                        chunk_idx += 1;
                    }
                    eprintln!(
                        "[operon] script upload chunks OK ({} chunks in {:?})",
                        chunk_idx,
                        t0.elapsed()
                    );
                    // Decode the assembled base64 and clean up
                    let decode_cmd = format!(
                        "base64 -d \"{}\" > \"{}\" && rm -f \"{}\"",
                        tmp_b64, escaped_script, tmp_b64,
                    );
                    crate::commands::ssh::ssh_exec(&profile, &decode_cmd).map_err(|e| {
                        eprintln!("[operon] script decode FAILED: {}", e);
                        format!("Failed to decode run script on remote: {}", e)
                    })?;
                    eprintln!("[operon] script decode OK");
                }
            }
            eprintln!(
                "[operon] writing source command to terminal id={:?}",
                terminal_id
            );

            // Clear the previous turn's transcript and marker HERE, synchronously,
            // before the agent is launched and before the tail is spawned.
            //
            // The run script clears them too, but that is not sufficient on its own:
            // the script runs on the compute node via the PTY while the tail is
            // spawned from here over a separate SSH connection, with no ordering
            // between them. `session_id` is reused for every turn of a conversation,
            // so from turn 2 onward the previous turn's .jsonl and .done are still
            // on disk — and a tail that wins the race sees the stale marker, replays
            // the whole previous transcript into the chat, and exits immediately.
            // (Before the tail stopped deleting these files that state could not
            // arise, so this race is new; doing the clear from Rust removes it.)
            {
                let clear = format!(
                    "rm -f -- '{}' '{}'",
                    output_file.replace('\'', "'\\''"),
                    done_file.replace('\'', "'\\''"),
                );
                if let Err(e) = super::ssh::ssh_exec(&profile, &clear) {
                    // Non-fatal: the run script's own `rm -f` still covers the common
                    // case. Log it so a stale-marker replay is diagnosable.
                    eprintln!("[operon] could not pre-clear session files: {e}");
                }
            }

            // Send a short source command to the terminal (the script is already on the remote)
            // The leading space prevents it from appearing in shell history.
            let terminal_cmd = format!(
                " clear; source '{}'; rm -f '{}'\n",
                script_file.replace('\'', "'\\''"),
                script_file.replace('\'', "'\\''"),
            );

            // Write the command into the existing terminal
            let encoded = terminal_cmd.as_bytes().to_vec();
            {
                let terminals = terminal_state.terminals.lock().map_err(|e| e.to_string())?;
                let handle = terminals
                    .get(tid)
                    .ok_or_else(|| {
                        eprintln!(
                            "[operon] terminal write FAILED: terminal id '{}' not found in TerminalManager (open terminals: {:?})",
                            tid,
                            terminals.keys().collect::<Vec<_>>()
                        );
                        format!("Terminal {} not found", tid)
                    })?;
                let mut writer = handle.writer.lock().map_err(|e| e.to_string())?;
                use std::io::Write;
                writer.write_all(&encoded).map_err(|e| {
                    eprintln!("[operon] terminal write FAILED on write_all: {}", e);
                    e.to_string()
                })?;
                writer.flush().map_err(|e| {
                    eprintln!("[operon] terminal write FAILED on flush: {}", e);
                    e.to_string()
                })?;
            }
            eprintln!(
                "[operon] terminal write OK ({} bytes to terminal id={})",
                encoded.len(),
                tid
            );

            // Now tail the output file via a separate SSH connection to stream results back.
            // Reuse ControlMaster socket if available — avoids re-authentication (critical
            // for HPC clusters with Duo MFA where a second auth would block/fail).
            let mut ssh_tail_args = format!(
                "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
                profile.user, profile.host, profile.port
            );
            // Reuse the ControlMaster socket from the main terminal connection,
            // but only if the master is actually live.
            if let Some(ctrl_sock) = super::ssh::live_control_socket(&profile) {
                ssh_tail_args.push_str(&format!(
                    " -o \"ControlPath={}\"",
                    ctrl_sock.to_string_lossy()
                ));
            }
            if let Some(key) = &profile.key_file {
                // Single-quote the key path: it runs through `bash -l -c`, and a
                // Windows path (C:\Users\...) would otherwise have its backslashes
                // eaten as shell escapes, breaking key auth.
                ssh_tail_args.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
            }
            // Wait for the output file to appear, then tail -f it.
            // Use base64 encoding to completely avoid all shell quoting/expansion issues
            // across the local shell → SSH → remote shell → bash -c chain.
            // The tail script streams the JSONL output file back to the local machine.
            // Key fixes for reliability:
            //   1. Use stdbuf/unbuffer to force line-buffered output through SSH pipe.
            //      Without this, SSH block-buffers stdout (4KB) so small JSON lines
            //      accumulate silently, causing the "thinking but not responding" symptom.
            //   2. Use tail --pid or a polling loop so tail exits promptly when done.
            //   3. Read any remaining lines after tail exits (tail -f may miss the last write).
            // Hardened so it can never become a long-lived orphan on the (login) host:
            //   - `trap '' HUP PIPE` so a dropped SSH connection doesn't kill the script
            //     before it can clean up its own `tail`/heartbeat children;
            //   - every loop iteration re-checks our PPID — once it becomes 1 the SSH
            //     parent is gone, so kill the children and exit immediately (<0.5s);
            //   - a hard 12h wall-clock cap as a last-resort backstop.
            // Early heartbeat + heartbeats during the file-wait loop so the
            // frontend's auto-reconnect watchdog doesn't fire while we're
            // legitimately waiting for the remote shell to create the output file
            // (slow Claude startup, NFS attribute cache, etc.). Without these, a
            // file that takes >60s to appear causes auto-reconnect to kill this
            // tail and spawn a new one — which finds the file briefly missing and
            // emits a spurious "Output file not found" error.
            let tail_script = format!(
                "trap '' HUP PIPE; \
                 printf '{{\"type\":\"heartbeat\"}}\\n'; \
                 i=0; while [ ! -f '{out}' ] && [ \"$i\" -lt 1500 ]; do sleep 0.2; i=$((i+1)); [ $((i % 50)) -eq 0 ] && printf '{{\"type\":\"heartbeat\"}}\\n'; done; \
                 if [ ! -f '{out}' ]; then echo '{{\"type\":\"error\",\"error\":{{\"message\":\"Output file did not appear after 5 minutes. The command may have failed to start — check the terminal.\"}}}}'; exit 1; fi; \
                 if command -v stdbuf >/dev/null 2>&1; then TAIL_CMD=\"stdbuf -oL tail -f '{out_esc}'\"; else TAIL_CMD=\"tail -f '{out_esc}'\"; fi; \
                 eval $TAIL_CMD & TAIL_PID=$!; \
                 ( _warned=0; while [ ! -f '{donef}' ]; do sleep 30; _pp=$(ps -o ppid= -p ${{BASHPID:-$$}} 2>/dev/null | tr -d ' '); [ \"$_pp\" = \"1\" ] && exit 0; printf '{{\"type\":\"heartbeat\"}}\\n'; if [ -f '{out}' ]; then _now=$(date +%s); _mt=$(stat -c %Y '{out}' 2>/dev/null || stat -f %m '{out}' 2>/dev/null || echo 0); if [ \"$_mt\" -gt 0 ]; then _age=$((_now - _mt)); if [ \"$_age\" -gt 300 ] && [ \"$_warned\" = \"0\" ]; then printf '{{\"type\":\"stall_warning\",\"message\":\"No output for over 5 minutes. This is normal for a long tool call, compile, download or queued job — the agent is still being watched. If the compute node was preempted or OOM-killed the session will end on its own.\"}}\\n'; _warned=1; fi; fi; fi; done ) & HB_PID=$!; \
                 _n=0; _capped=0; while [ ! -f '{donef}' ]; do sleep 0.5; _n=$((_n+1)); [ \"$_n\" -ge 86400 ] && {{ _capped=1; break; }}; _pp=$(ps -o ppid= -p $$ 2>/dev/null | tr -d ' '); [ \"$_pp\" = \"1\" ] && {{ kill $TAIL_PID $HB_PID 2>/dev/null; exit 0; }}; done; \
                 sleep 0.5; kill $TAIL_PID $HB_PID 2>/dev/null; wait $TAIL_PID $HB_PID 2>/dev/null; \
                 [ \"$_capped\" = \"1\" ] && exit 0; \
                 cat '{out_esc}'",
                out = output_file,
                out_esc = output_file.replace('\'', "'\\''"),
                donef = done_file,
            );
            // Base64-encode the script and have the REMOTE shell decode+execute it.
            // This avoids ALL quoting issues: local shell sees only safe base64 chars.
            let b64_tail = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());
            // The remote command: echo <b64> | base64 -d | bash
            // We pass this directly to SSH (no -- bash -c wrapper needed).
            // SSH sends its args as a single command string to the remote shell.
            ssh_tail_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64_tail));

            let mut tail_cmd = AsyncCommand::new(&shell);
            tail_cmd.arg("-l").arg("-c").arg(&ssh_tail_args);
            let tail_provider_env = ai_provider_env(&settings_state, &proxy_state, &api_key);
            for k in ai_provider_env_unset(&tail_provider_env) {
                tail_cmd.env_remove(k);
            }
            for (k, v) in tail_provider_env {
                tail_cmd.env(k, v);
            }
            tail_cmd.stdout(std::process::Stdio::piped());
            tail_cmd.stderr(std::process::Stdio::piped());
            hide_window_async(&mut tail_cmd);

            let mut child = tail_cmd
                .spawn()
                .map_err(|e| format!("Failed to start tail: {}", e))?;
            let stdout = child.stdout.take().ok_or("Failed to capture tail stdout")?;
            let stderr = child.stderr.take();

            // Store as a session so it can be stopped
            state
                .sessions
                .lock()
                .map_err(|e| e.to_string())?
                .insert(session_id.clone(), ClaudeSession { child });

            // Stream stdout (JSON lines from the output file)
            let app_handle = app.clone();
            let sid = session_id.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let _ = app_handle.emit(
                        &format!("claude-event-{}", sid),
                        serde_json::json!({ "line": line }),
                    );
                }
                let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
            });

            // Handle stderr (suppress SSH warnings)
            if let Some(stderr) = stderr {
                let app_handle2 = app.clone();
                let sid2 = session_id.clone();
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    let mut error_buf = String::new();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if !line.trim().is_empty() {
                            error_buf.push_str(&line);
                            error_buf.push('\n');
                        }
                    }
                    let trimmed = error_buf.trim();
                    if !trimmed.is_empty() {
                        let is_just_warning = trimmed.lines().all(|l| {
                            let lt = l.trim().trim_start_matches('*').trim();
                            lt.is_empty()
                                || lt.contains("WARNING")
                                || lt.contains("Warning")
                                || lt.contains("warning")
                                || lt.contains("sntrup")
                                || lt.contains("mlkem")
                                || lt.contains("post-quantum")
                                || lt.contains("quantum")
                                || lt.contains("vulnerable")
                                || lt.contains("decrypt later")
                                || lt.contains("upgraded")
                                || lt.contains("openssh.com")
                                || lt.contains("store now")
                                || lt.contains("key exchange")
                                || lt.contains("no stdin data")
                                || lt.contains("redirect stdin")
                                || lt.contains("piping from")
                                || lt.contains("/dev/null")
                                || lt.contains("wait longer")
                                || lt.contains("proceeding without")
                                || lt.contains("Connection to")
                                || lt.contains("Killed by signal")
                                || lt.contains("Transferred:")
                                || lt.contains("kex_exchange")
                                || lt.contains("banner")
                                || lt.starts_with("debug")
                                || lt.contains("file truncated")
                                || lt.contains("tail:")
                        });
                        if !is_just_warning {
                            let _ = app_handle2.emit(
                                &format!("claude-event-{}", sid2),
                                serde_json::json!({
                                    "line": format!(
                                        "{{\"type\":\"error\",\"error\":{{\"message\":\"{}\"}}}}",
                                        trimmed.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                                    )
                                }),
                            );
                        }
                    }
                });
            }

            return Ok(());
        } else {
            return Err(
                "Terminal mode requires a remote connection and an active terminal".to_string(),
            );
        }
    }

    // The credential set for this spawn. Computed once so the shell-level
    // `unset` prefix built inside the local branch and the process-level
    // `cmd.env` / `cmd.env_remove` applied after it are derived from the same
    // decision — they must never disagree about which vars we own.
    let spawn_provider_env = ai_provider_env(&settings_state, &proxy_state, &api_key);

    // Decide: local or remote execution
    let mut cmd = if let Some(ref ctx) = remote {
        // --- REMOTE: run claude via SSH on the remote server ---
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };

        // Step 1: Figure out how to invoke claude on the remote server.
        // It might be: a binary in PATH, an alias (e.g. alias claude='npx @anthropic-ai/claude-code'),
        // or available via npx. We detect all cases and return the actual invocation command.
        let find_claude_cmd = r#"
            # 1. Check for a real binary at common install locations
            for p in \
                "$HOME/.local/bin/claude" \
                "$HOME/.npm-global/bin/claude" \
                "$HOME/.npm/bin/claude" \
                "$HOME/bin/claude" \
                "$HOME/.yarn/bin/claude" \
                "$HOME/.bun/bin/claude" \
                /usr/local/bin/claude; do
                [ -x "$p" ] && echo "$p" && exit 0
            done
            # Check NVM paths
            for p in "$HOME"/.nvm/versions/node/*/bin/claude; do
                [ -x "$p" ] && echo "$p" && exit 0
            done

            # 2. Source profile files to get aliases and full PATH
            # Set PS1 to trick .bashrc into thinking this is interactive
            # (most .bashrc files have: [ -z "$PS1" ] && return)
            # Also enable alias expansion so `alias` builtin works after sourcing
            export PS1=x
            shopt -s expand_aliases 2>/dev/null
            . "$HOME/.profile" 2>/dev/null
            . "$HOME/.bash_profile" 2>/dev/null
            . "$HOME/.bashrc" 2>/dev/null
            . "$HOME/.nvm/nvm.sh" 2>/dev/null

            # 3. Check if claude is a real binary via which
            w=$(which claude 2>/dev/null)
            if [ -n "$w" ] && [ -x "$w" ]; then
                echo "$w"
                exit 0
            fi

            # 4. Check if claude is an alias — extract the underlying command
            a=$(alias claude 2>/dev/null)
            if [ -n "$a" ]; then
                # alias output: alias claude='npx @anthropic-ai/claude-code'
                # Extract the command between quotes
                cmd=$(echo "$a" | sed "s/^[^']*'//;s/'[^']*$//")
                if [ -n "$cmd" ]; then
                    echo "ALIAS:$cmd"
                    exit 0
                fi
            fi

            # 5. Check if npx can run it directly
            npx_path=$(which npx 2>/dev/null)
            if [ -n "$npx_path" ]; then
                echo "ALIAS:$npx_path @anthropic-ai/claude-code"
                exit 0
            fi

            echo ""
        "#;
        let claude_resolve = super::ssh::ssh_exec(&profile, find_claude_cmd).unwrap_or_default();
        let claude_resolve = claude_resolve.trim().to_string();

        if claude_resolve.is_empty() || claude_resolve.contains("not found") {
            return Err("Claude CLI not found on the remote server. \
                        Install it with: curl -fsSL https://claude.ai/install.sh | bash"
                .to_string());
        }

        // Step 2: Replace `claude` with the resolved command
        // If it starts with "ALIAS:", it's a multi-word command (e.g. "npx @anthropic-ai/claude-code")
        // Otherwise it's an absolute binary path
        let claude_invoke = if let Some(alias_cmd) = claude_resolve.strip_prefix("ALIAS:") {
            alias_cmd.trim().to_string()
        } else {
            claude_resolve.clone()
        };

        // For report mode, upload the prompt file to the remote server via SCP
        if mode == "report" {
            let local_prompt_file = std::env::temp_dir()
                .join(format!("operon-report-prompt-{}.txt", session_id))
                .to_string_lossy()
                .to_string();
            let remote_prompt_file = format!(
                "{}/.operon-report-prompt-{}.txt",
                ctx.remote_path, session_id
            );
            if std::path::Path::new(&local_prompt_file).exists() {
                let host_str = format!("{}@{}", profile.user, profile.host);
                let mut scp_args: Vec<String> = vec![
                    "-o".to_string(),
                    "BatchMode=yes".to_string(),
                    "-o".to_string(),
                    "ConnectTimeout=10".to_string(),
                ];
                if let Some(sock) = super::ssh::live_control_socket(&profile) {
                    scp_args.push("-o".to_string());
                    scp_args.push(format!("ControlPath={}", sock.to_string_lossy()));
                }
                if profile.port != 22 {
                    scp_args.push("-P".to_string());
                    scp_args.push(profile.port.to_string());
                }
                if let Some(key) = &profile.key_file {
                    if std::path::Path::new(key).exists() {
                        scp_args.push("-i".to_string());
                        scp_args.push(key.clone());
                    }
                }
                scp_args.push(local_prompt_file.clone());
                scp_args.push(format!("{}:{}", host_str, remote_prompt_file));

                match hide_window(std::process::Command::new("scp").args(&scp_args)).output() {
                    Ok(output) if output.status.success() => {
                        let file_size = std::fs::metadata(&local_prompt_file)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        eprintln!(
                            "[operon] SCP uploaded report prompt: {} ({} bytes)",
                            remote_prompt_file, file_size
                        );
                    }
                    Ok(output) => {
                        eprintln!(
                            "[operon] SCP upload failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    Err(e) => {
                        eprintln!("[operon] SCP command failed: {}", e);
                    }
                }
                claude_cmd = claude_cmd.replace(&local_prompt_file, &remote_prompt_file);
            }
        }

        // Install the agent file-deletion guard on the remote and point claude at it.
        let guard_block =
            remote_guard_setup_block(&review_guard_cfg(&settings_state, &session_id, None));
        claude_cmd.push_str(" --settings \"$HOME/.operon/guard/operon-guard-settings.json\"");
        let claude_cmd_abs = claude_cmd.replacen("claude ", &format!("{} ", claude_invoke), 1);

        // Step 3: Build the remote command — source profile for PATH (needed for npx/node)
        // then cd to the working directory and run claude
        // For report mode, the command is `cat file | claude ...` — don't redirect stdin from /dev/null.
        // For other modes, redirect stdin to prevent Claude from hanging waiting for input.
        let stdin_redirect = if mode == "report" { "" } else { " < /dev/null" };
        // Forward provider env vars to the remote command — SSH doesn't forward
        // env vars by default, and HPC servers rarely have AcceptEnv configured
        // for custom vars. For custom endpoints this forwards ANTHROPIC_BASE_URL
        // + ANTHROPIC_AUTH_TOKEN so the remote Claude hits the same proxy.
        let api_key_export = ai_provider_env_exports(
            &spawn_provider_env,
            &ai_provider_env_unset(&spawn_provider_env),
        );
        let remote_cmd = format!(
            "export PS1=x; . \"$HOME/.profile\" 2>/dev/null; . \"$HOME/.bash_profile\" 2>/dev/null; . \"$HOME/.bashrc\" 2>/dev/null; . \"$HOME/.nvm/nvm.sh\" 2>/dev/null; {}{}cd '{}' && {}{}",
            api_key_export,
            guard_block,
            ctx.remote_path.replace('\'', "'\\''"),
            claude_cmd_abs,
            stdin_redirect
        );

        // Base64-encode to avoid nested quoting issues
        let encoded_cmd = base64::engine::general_purpose::STANDARD.encode(remote_cmd.as_bytes());

        // No -tt flag! We need clean stdout for JSON parsing, not a PTY.
        let mut ssh_args = format!(
            "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
            profile.user, profile.host, profile.port
        );
        // Reuse ControlMaster socket if the master is live (avoids re-auth on Duo MFA clusters)
        if let Some(ctrl_sock) = super::ssh::live_control_socket(&profile) {
            ssh_args.push_str(&format!(
                " -o \"ControlPath={}\"",
                ctrl_sock.to_string_lossy()
            ));
        }
        if let Some(key) = &profile.key_file {
            // Single-quote the key path: it runs through `bash -l -c`, and a
            // Windows path (C:\Users\...) would otherwise have its backslashes
            // eaten as shell escapes, breaking key auth.
            ssh_args.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
        }
        // Decode and execute on the remote side
        ssh_args.push_str(&format!(
            " -- bash -c \"$(echo {} | base64 -d)\"",
            encoded_cmd
        ));

        let mut c = AsyncCommand::new(&shell);
        // On Windows the ssh invocation embeds the base64 of the remote command
        // (which contains the full prompt) inline. With a large prompt — e.g.
        // attached protocols — that overflows Windows' ~32 KB CreateProcess
        // command-line limit and spawning fails with "os error 206". Write the
        // ssh command to a temp script file and run it from there so the command
        // line stays tiny. macOS/Linux have a much larger ARG_MAX, so they pass
        // the command directly.
        #[cfg(windows)]
        {
            let script_file = std::env::temp_dir().join(format!("operon-ssh-{}.sh", session_id));
            std::fs::write(&script_file, ssh_args.as_bytes())
                .map_err(|e| format!("Failed to write ssh command script: {}", e))?;
            let script_str = script_file.to_string_lossy().to_string();
            let script_posix = crate::platform::common::normalize_display_path(&script_str);
            c.arg("-l")
                .arg("-c")
                .arg(format!("bash -l '{}'", script_posix.replace('\'', "'\\''")));
        }
        #[cfg(not(windows))]
        {
            c.arg("-l").arg("-c").arg(&ssh_args);
        }
        c
    } else {
        // --- LOCAL: run claude directly ---
        // Install the agent file-deletion guard (writes ~/.operon/guard, appends
        // --settings). No-op on Windows.
        install_local_guard(
            &mut claude_cmd,
            &review_guard_cfg(
                &settings_state,
                &session_id,
                crate::platform::resolve_claude_path(),
            ),
        );
        // Prepend `unset <provider-conflicting vars>` so any stale value re-exported
        // by the user's shell profile (~/.zshrc, ~/.bash_profile) inside `bash -l`
        // is cleared BEFORE claude runs. Without this, e.g. an
        // `export ANTHROPIC_BASE_URL=https://api.portkey.ai/v1` left over in
        // .zshrc routes "Anthropic-direct" sessions to Portkey and produces the
        // confusing "x-portkey-provider header required" 400 — or, for a
        // subscription user, a leftover `export ANTHROPIC_API_KEY=...` outranks
        // their claude.ai login and every session fails with "Invalid API key".
        //
        // The vars we DO supply are re-exported here too, from the staging vars
        // set via `cmd.env` below — `cmd.env` alone loses to the profile, since
        // `bash -l` sources it after the process starts. The unset list is the
        // complement of what we export, so the two can never collide.
        let provider_unset = ai_provider_env_unset(&spawn_provider_env);
        let unset_prefix = local_env_prefix(&spawn_provider_env, &provider_unset);
        // Pin `claude` to its absolute path (Unix) so a Finder/Dock launch — whose
        // login shell can't see ~/.local/bin — still finds it. No-op on Windows.
        let claude_cmd = substitute_claude_invocation(&claude_cmd);
        let wrapped_cmd = format!("{}{}", unset_prefix, claude_cmd);
        let mut c = AsyncCommand::new(&shell);
        // On Windows the shell is Git Bash (MSYS2). It re-parses the Windows
        // command line to rebuild argv, which mangles a complex `-c` string — the
        // prompt and appended system prompt contain both single and double quotes,
        // producing "unexpected EOF while looking for matching" and failing local
        // chat. We write the full command to a temp script file and run it with a
        // fresh login bash, so the outer `-c` arg holds only a short, quote-free
        // file path and MSYS has nothing to mangle.
        //
        // This ALSO fixes "os error 206" (ERROR_FILENAME_EXCED_RANGE): the prior
        // approach embedded the base64 of the command inline (`echo <b64> | ...`),
        // which — with a large prompt such as attached protocols — overflowed
        // Windows' ~32 KB CreateProcess command-line limit and failed to spawn.
        // A file path keeps the command line tiny no matter how big the prompt is.
        // macOS/Linux parse argv once (and have a much larger ARG_MAX), so they
        // run the command as-is.
        #[cfg(windows)]
        {
            let script_file = std::env::temp_dir().join(format!("operon-cmd-{}.sh", session_id));
            std::fs::write(&script_file, wrapped_cmd.as_bytes())
                .map_err(|e| format!("Failed to write command script: {}", e))?;
            let script_str = script_file.to_string_lossy().to_string();
            let script_posix = crate::platform::common::normalize_display_path(&script_str);
            c.arg("-l")
                .arg("-c")
                .arg(format!("bash -l '{}'", script_posix.replace('\'', "'\\''")));
        }
        #[cfg(not(windows))]
        {
            c.arg("-l").arg("-c").arg(&wrapped_cmd);
        }
        c.current_dir(&project_path);
        c
    };

    // Belt-and-suspenders: also strip the conflicting vars from the inherited
    // env we hand to bash, so even profile-less spawns (`sh -c` style) stay clean.
    // Remove first, then set — the two lists are disjoint, but ordering it this
    // way makes a future overlap fail closed (we keep our value) instead of
    // silently deleting the credential we just supplied.
    for k in ai_provider_env_unset(&spawn_provider_env) {
        cmd.env_remove(k);
    }
    for (k, v) in spawn_provider_env {
        // Staged copy under OPERON_ENV_<NAME>: the local shell prefix re-exports
        // from it so our value wins over the user's profile. Remote sessions
        // ignore these — they carry their own `export`s in the remote command.
        cmd.env(format!("{}{}", LOCAL_ENV_STAGE_PREFIX, k), &v);
        cmd.env(k, v);
    }

    // On Windows, Claude Code requires Git Bash. Set the path so it can find it.
    if let Some(git_bash_path) = crate::platform::find_git_bash_path() {
        cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
    }

    // Make sure /usr/bin etc. are on PATH before bash sources the user's
    // login profile (~/.bash_profile). Without this, profiles that call
    // coreutils like `id` emit "command not found" to stderr at startup
    // and Operon's stderr-reader misreports it as a fatal session error.
    enforce_baseline_path(&mut cmd);

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    hide_window_async(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start Claude: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let stderr = child.stderr.take();

    // Store session
    state
        .sessions
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id.clone(), ClaudeSession { child });

    // Spawn stdout reader task
    let app_handle = app.clone();
    let sid = session_id.clone();
    // Persist output to .jsonl file so sessions can be resumed/reconnected.
    // For local sessions this was previously missing — output was only streamed live.
    let output_jsonl_path = format!("{}/.operon-{}.jsonl", project_path, session_id);
    let done_marker_path = format!("{}/.operon-{}.done", project_path, session_id);

    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        // Open the output file for appending (create if needed)
        let mut output_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_jsonl_path)
            .await
            .ok();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            // Emit the raw JSON line to frontend for parsing
            let _ = app_handle.emit(
                &format!("claude-event-{}", sid),
                serde_json::json!({ "line": line }),
            );

            // Persist to disk for session resume
            if let Some(ref mut f) = output_file {
                use tokio::io::AsyncWriteExt;
                let _ = f.write_all(line.as_bytes()).await;
                let _ = f.write_all(b"\n").await;
            }
        }

        // Stream ended — write done marker and emit event
        let _ = tokio::fs::write(&done_marker_path, "done").await;
        let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
    });

    // Spawn stderr reader task — surface SSH/remote errors to the frontend
    if let Some(stderr) = stderr {
        let app_handle2 = app.clone();
        let sid2 = session_id.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut error_buf = String::new();

            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    error_buf.push_str(&line);
                    error_buf.push('\n');
                }
            }

            // If there was meaningful stderr output, send it as an error event
            let trimmed = error_buf.trim();
            if !trimmed.is_empty() {
                // Filter out common SSH warnings (post-quantum key exchange, etc.)
                let is_just_warning = trimmed.lines().all(|l| {
                    let lt = l.trim().trim_start_matches('*').trim();
                    lt.is_empty()
                        || lt.contains("WARNING")
                        || lt.contains("Warning")
                        || lt.contains("warning")
                        || lt.contains("sntrup")
                        || lt.contains("mlkem")
                        || lt.contains("post-quantum")
                        || lt.contains("quantum")
                        || lt.contains("vulnerable")
                        || lt.contains("decrypt later")
                        || lt.contains("upgraded")
                        || lt.contains("openssh.com")
                        || lt.contains("store now")
                        || lt.contains("key exchange")
                        || lt.contains("no stdin data")
                        || lt.contains("redirect stdin")
                        || lt.contains("piping from")
                        || lt.contains("/dev/null")
                        || lt.contains("wait longer")
                        || lt.contains("proceeding without")
                        || lt.contains("file truncated")
                        || lt.contains("tail:")
                        // Profile-sourcing complaints from a user's
                        // .bash_profile that calls a coreutil (id, uname,
                        // stat, tput, etc.) before PATH is set up. We
                        // already prepend a baseline PATH in
                        // enforce_baseline_path() so these should rarely
                        // fire, but allow them through as warnings if they
                        // do. Match the exact "bash: line N: <util>: ..."
                        // shape to avoid masking real "claude not found"
                        // or "node not found" failures.
                        || is_profile_coreutil_warning(lt)
                });

                if !is_just_warning {
                    let _ = app_handle2.emit(
                        &format!("claude-event-{}", sid2),
                        serde_json::json!({
                            "line": format!(
                                "{{\"type\":\"error\",\"error\":{{\"message\":\"{}\"}}}}",
                                trimmed.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                            )
                        }),
                    );
                }
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_claude_session(
    state: tauri::State<'_, ClaudeManager>,
    terminal_state: tauri::State<'_, super::terminal::TerminalManager>,
    session_id: String,
) -> Result<(), String> {
    // Extract session from lock first, then await kill — never hold Mutex across .await.
    // For remote (HPC) sessions this kills the local SSH process; the remote tail
    // script is hardened to notice its parent went away (PPID==1) and reap its own
    // `tail`/heartbeat children within ~0.5s, so nothing is left on the login node.
    let session = {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.remove(&session_id)
    };

    if let Some(mut session) = session {
        let _ = session.child.kill().await;
    }

    // Killing the tracked child is NOT enough in terminal mode. There the tracked
    // child is the login-node SSH tail; the agent itself was typed into the user's
    // tmux pane and keeps running on the compute node — still editing files, still
    // burning node hours, and still owning the pane, so the next turn's `source`
    // line would be typed into Claude's stdin instead of a shell. Deliver a real
    // interrupt to the pane.
    //
    // Best-effort by construction: the terminal tab may have been closed, or this
    // may be a local session with no terminal at all. Stop must never fail because
    // the interrupt could not be delivered.
    if let Ok(Some(meta)) = load_session_from_disk(&session_id) {
        if meta.use_terminal {
            if let Some(tid) = meta.terminal_id.as_deref() {
                // ETX (Ctrl-C) — what the user would press themselves.
                if let Err(e) = super::terminal::write_terminal_bytes(&terminal_state, tid, b"\x03")
                {
                    eprintln!("[operon] stop: could not interrupt terminal {tid}: {e}");
                }
            }
        }
        // Record the stop server-side so a late `done` event cannot resurrect the
        // session as "completed" in the history/resume UI.
        let mut meta = meta;
        meta.status = "stopped".to_string();
        let _ = save_session_to_disk(&meta);
    }

    Ok(())
}

/// Check if an implementation_plan.md exists in the given directory (local or remote).
/// Returns the plan content if found, or an empty string if not.
#[tauri::command]
pub async fn check_existing_plan(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    project_path: String,
    remote: Option<RemoteContext>,
) -> Result<String, String> {
    if let Some(ctx) = remote {
        // Remote: check via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };
        let check_cmd = format!(
            "cat '{}'/implementation_plan.md 2>/dev/null || echo ''",
            ctx.remote_path.replace('\'', "'\\''")
        );
        let content = super::ssh::ssh_exec(&profile, &check_cmd).unwrap_or_default();
        Ok(content.trim().to_string())
    } else {
        // Local
        let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
        let content = std::fs::read_to_string(&plan_path).unwrap_or_default();
        Ok(content.trim().to_string())
    }
}

/// Archive the current implementation_plan.md to .operon/plan_history/ before a new plan is written.
/// Called by the frontend before starting a plan session, so archival happens regardless of
/// what mode string the backend receives.
/// Returns Ok(true) if a plan was archived, Ok(false) if there was no plan to archive.
#[tauri::command]
pub async fn archive_current_plan(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    project_path: String,
    remote: Option<RemoteContext>,
) -> Result<bool, String> {
    // Generate timestamp for the archive filename
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    for &md in &month_days {
        if remaining < md as i64 {
            break;
        }
        remaining -= md as i64;
        m += 1;
    }
    let ts = format!(
        "{:04}-{:02}-{:02}_{:02}{:02}{:02}_UTC",
        y,
        m + 1,
        remaining + 1,
        hours,
        minutes,
        seconds
    );

    if let Some(ctx) = remote {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
        };
        if let Some(prof) = profile {
            let base = ctx.remote_path.replace('\'', "'\\''");
            // Check if plan exists, archive it, then return
            let cmd = format!(
                "if [ -f '{base}/implementation_plan.md' ]; then \
                     mkdir -p '{base}/.operon/plan_history' && \
                     cp '{base}/implementation_plan.md' '{base}/.operon/plan_history/plan_{ts}.md' && \
                     echo 'ARCHIVED'; \
                 else echo 'NO_PLAN'; fi"
            );
            let result = super::ssh::ssh_exec(&prof, &cmd).unwrap_or_default();
            return Ok(result.contains("ARCHIVED"));
        }
        Ok(false)
    } else {
        let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
        if plan_path.is_file() {
            let history_dir = std::path::Path::new(&project_path)
                .join(".operon")
                .join("plan_history");
            std::fs::create_dir_all(&history_dir)
                .map_err(|e| format!("Failed to create plan_history dir: {}", e))?;
            let archive_name = format!("plan_{}.md", ts);
            std::fs::copy(&plan_path, history_dir.join(&archive_name))
                .map_err(|e| format!("Failed to archive plan: {}", e))?;
            eprintln!(
                "[operon] Archived implementation_plan.md → .operon/plan_history/{}",
                archive_name
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Archived plan entry returned to the frontend.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct PlanHistoryEntry {
    pub filename: String,
    pub timestamp: String, // e.g. "2026-03-29 14:30:05"
    pub title: String,     // first heading or "Untitled Plan"
    pub lines: u64,
    pub path: String, // full path to the archived file
}

/// List all archived plans from .operon/plan_history/, newest first.
#[tauri::command]
pub async fn list_plan_history(project_path: String) -> Result<Vec<PlanHistoryEntry>, String> {
    let history_dir = std::path::Path::new(&project_path)
        .join(".operon")
        .join("plan_history");
    if !history_dir.is_dir() {
        return Ok(vec![]);
    }

    let mut entries: Vec<PlanHistoryEntry> = Vec::new();
    let dir = std::fs::read_dir(&history_dir).map_err(|e| e.to_string())?;
    for entry in dir.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if !fname.starts_with("plan_") || !fname.ends_with(".md") {
            continue;
        }
        // Parse timestamp from filename: plan_YYYY-MM-DD_HHMMSS.md
        let ts_part = fname.trim_start_matches("plan_").trim_end_matches(".md");
        let timestamp = ts_part
            .replacen('_', " ", 1) // "2026-03-29 143005"
            .chars()
            .enumerate()
            .map(|(i, c)| {
                // Insert colons into HHMMSS → HH:MM:SS
                if i == 13 || i == 15 {
                    ':'
                } else {
                    c
                }
            })
            .collect::<String>();

        let full_path = entry.path();
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let line_count = content.lines().count() as u64;

        // Extract title from first heading
        let title = content
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").trim().to_string())
            .unwrap_or_else(|| "Untitled Plan".to_string());

        entries.push(PlanHistoryEntry {
            filename: fname,
            timestamp,
            title,
            lines: line_count,
            path: full_path.to_string_lossy().to_string(),
        });
    }

    // Sort newest first
    entries.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(entries)
}

/// Read the content of a specific archived plan.
#[tauri::command]
pub async fn read_plan_history_entry(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read plan: {}", e))
}

// --- Session Management Commands ---

/// Save session metadata to disk. Called by frontend after session starts or updates.
#[tauri::command]
pub async fn save_session_metadata(metadata: SessionMetadata) -> Result<(), String> {
    save_session_to_disk(&metadata)
}

/// Update the claude_session_id for an existing session (called when we capture it from stream).
#[tauri::command]
pub async fn update_session_claude_id(
    session_id: String,
    claude_session_id: String,
) -> Result<(), String> {
    if let Some(mut meta) = load_session_from_disk(&session_id)? {
        meta.claude_session_id = Some(claude_session_id);
        meta.last_activity = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        save_session_to_disk(&meta)
    } else {
        Err(format!("Session {} not found", session_id))
    }
}

/// Mark a session as completed or failed.
/// Whether a session's persisted status may move from `from` to `to`.
///
/// `stopped` is terminal. Killing the tracked child is exactly what makes the
/// stdout task end and emit `claude-done-{id}`, whose frontend handler writes
/// "completed" — so without this guard every Stop was immediately overwritten and
/// the session showed as completed in history and the resume UI. The ordering is
/// stopped → completed by construction, not by an unlucky race.
pub(crate) fn status_transition_allowed(from: &str, to: &str) -> bool {
    !(from == "stopped" && to != "stopped")
}

#[tauri::command]
pub async fn update_session_status(session_id: String, status: String) -> Result<(), String> {
    if let Some(mut meta) = load_session_from_disk(&session_id)? {
        if !status_transition_allowed(&meta.status, &status) {
            return Ok(());
        }
        meta.status = status;
        meta.last_activity = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        save_session_to_disk(&meta)
    } else {
        Err(format!("Session {} not found", session_id))
    }
}

/// List sessions for a given project path (local or remote).
/// Returns sessions sorted by most recent first.
#[tauri::command]
pub async fn list_sessions(
    project_path: Option<String>,
    profile_id: Option<String>,
) -> Result<Vec<SessionMetadata>, String> {
    let all = load_all_sessions_from_disk();
    let filtered: Vec<SessionMetadata> = all
        .into_iter()
        .filter(|s| {
            // Filter by project path or profile if provided
            let path_match = project_path.as_ref().is_none_or(|p| {
                s.project_path == *p || s.remote_path.as_deref() == Some(p.as_str())
            });
            let profile_match = profile_id
                .as_ref()
                .is_none_or(|pid| s.profile_id.as_deref() == Some(pid.as_str()));
            path_match && profile_match
        })
        .collect();
    Ok(filtered)
}

/// Check the status of a session's output files on the filesystem (local or remote).
#[tauri::command]
pub async fn check_session_files(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    session_id: String,
    remote: Option<RemoteContext>,
) -> Result<SessionFileStatus, String> {
    // Load session metadata to find the output file path
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
    let done_file = format!("{}/.operon-{}.done", base_path, session_id);

    if let Some(ctx) = remote {
        // Remote: check via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };
        let check_cmd = format!(
            "echo -n \"output:\"; test -f '{}' && echo 'yes' || echo 'no'; \
             echo -n \"done:\"; test -f '{}' && echo 'yes' || echo 'no'",
            output_file.replace('\'', "'\\''"),
            done_file.replace('\'', "'\\''"),
        );
        let result = super::ssh::ssh_exec(&profile, &check_cmd).unwrap_or_default();
        let output_exists = result.contains("output:yes");
        let done_exists = result.contains("done:yes");
        Ok(SessionFileStatus {
            session_id,
            output_exists,
            done_exists,
            is_running: output_exists && !done_exists,
            is_completed: output_exists && done_exists,
        })
    } else {
        // Local
        let output_exists = std::path::Path::new(&output_file).exists();
        let done_exists = std::path::Path::new(&done_file).exists();
        Ok(SessionFileStatus {
            session_id,
            output_exists,
            done_exists,
            is_running: output_exists && !done_exists,
            is_completed: output_exists && done_exists,
        })
    }
}

/// Read the full output of a completed session (.jsonl file).
/// Returns the raw content for the frontend to parse into messages.
#[tauri::command]
pub async fn read_session_output(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    session_id: String,
    remote: Option<RemoteContext>,
) -> Result<String, String> {
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);

    if let Some(ctx) = remote {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };
        let cat_cmd = format!("cat '{}'", output_file.replace('\'', "'\\''"));
        let content = super::ssh::ssh_exec(&profile, &cat_cmd)
            .map_err(|e| format!("Failed to read session output: {}", e))?;
        Ok(content)
    } else {
        std::fs::read_to_string(&output_file)
            .map_err(|e| format!("Failed to read session output: {}", e))
    }
}

/// Reconnect to a running session by tailing the .jsonl file.
/// This spawns a tail process and streams events back to the frontend.
#[tauri::command]
pub async fn reconnect_session(
    state: tauri::State<'_, ClaudeManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    app: tauri::AppHandle,
    session_id: String,       // The old session's ID (to find the files)
    event_session_id: String, // The current frontend session ID (for event channels)
    remote: Option<RemoteContext>,
) -> Result<(), String> {
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
    let done_file = format!("{}/.operon-{}.done", base_path, session_id);

    let shell = claude_shell()?;

    if let Some(ctx) = remote {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };

        // Build SSH command to tail the output file
        let mut ssh_tail_args = format!(
            "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
            profile.user, profile.host, profile.port
        );
        if let Some(key) = &profile.key_file {
            // Single-quote the key path: it runs through `bash -l -c`, and a
            // Windows path (C:\Users\...) would otherwise have its backslashes
            // eaten as shell escapes, breaking key auth.
            ssh_tail_args.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
        }

        // Tail script: first cat any existing content, then tail -f for new lines
        // If done file already exists, just cat and exit (session already finished).
        // Hardened against orphaning: trap HUP/PIPE, self-destruct once PPID==1, 12h cap.
        // Wait up to 5 min for the file (same as the main start path) before
        // declaring it missing — covers NFS attribute-cache lag and slow startup.
        // Heartbeats during the wait prevent the frontend's auto-reconnect from
        // firing AGAIN while we're waiting (which would kill this tail and loop).
        let tail_script = format!(
            "trap '' HUP PIPE; \
             printf '{{\"type\":\"heartbeat\"}}\\n'; \
             if [ -f '{donef}' ]; then cat '{out}'; exit 0; fi; \
             i=0; while [ ! -f '{out}' ] && [ \"$i\" -lt 1500 ]; do sleep 0.2; i=$((i+1)); [ $((i % 50)) -eq 0 ] && printf '{{\"type\":\"heartbeat\"}}\\n'; done; \
             if [ ! -f '{out}' ]; then echo '{{\"type\":\"error\",\"error\":{{\"message\":\"Output file did not appear after 5 minutes — the remote command may have failed to start. Check the terminal for errors.\"}}}}'; exit 1; fi; \
             cat '{out}'; tail -f -n +$(wc -l < '{out}' | tr -d ' ') '{out}' & TAIL_PID=$!; \
             _n=0; while [ ! -f '{donef}' ]; do sleep 1; _n=$((_n+1)); [ \"$_n\" -ge 43200 ] && break; _pp=$(ps -o ppid= -p $$ 2>/dev/null | tr -d ' '); [ \"$_pp\" = \"1\" ] && {{ kill $TAIL_PID 2>/dev/null; exit 0; }}; done; \
             sleep 1; kill $TAIL_PID 2>/dev/null; wait $TAIL_PID 2>/dev/null",
            donef = done_file, out = output_file,
        );
        let b64_tail = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());
        ssh_tail_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64_tail));

        let mut tail_cmd = AsyncCommand::new(&shell);
        tail_cmd.arg("-l").arg("-c").arg(&ssh_tail_args);
        tail_cmd.stdout(std::process::Stdio::piped());
        tail_cmd.stderr(std::process::Stdio::piped());
        hide_window_async(&mut tail_cmd);

        let mut child = tail_cmd
            .spawn()
            .map_err(|e| format!("Failed to reconnect: {}", e))?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture reconnect stdout")?;

        // Store as a session so it can be stopped
        state
            .sessions
            .lock()
            .map_err(|e| e.to_string())?
            .insert(event_session_id.clone(), ClaudeSession { child });

        // Stream output to frontend using the CURRENT frontend session ID for events
        let app_handle = app.clone();
        let sid = event_session_id.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let _ = app_handle.emit(
                    &format!("claude-event-{}", sid),
                    serde_json::json!({ "line": line }),
                );
            }
            let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
        });

        Ok(())
    } else {
        // Local reconnect — just read the file
        let content = std::fs::read_to_string(&output_file)
            .map_err(|e| format!("Failed to read output: {}", e))?;
        for line in content.lines() {
            if !line.trim().is_empty() {
                let _ = app.emit(
                    &format!("claude-event-{}", event_session_id),
                    serde_json::json!({ "line": line }),
                );
            }
        }
        let _ = app.emit(
            &format!("claude-done-{}", event_session_id),
            serde_json::json!({}),
        );
        Ok(())
    }
}

/// Reconnect a stalled tail SSH connection without stopping the agent.
/// Kills the existing tail process for this session and spawns a fresh one
/// that cats existing output then tail -f's for new lines.
#[tauri::command]
pub async fn reconnect_tail(
    state: tauri::State<'_, ClaudeManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    app: tauri::AppHandle,
    session_id: String,
    remote: Option<RemoteContext>,
) -> Result<(), String> {
    // 1. Kill the stalled tail process (but NOT the agent on the compute node)
    let old_session = {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.remove(&session_id)
    };
    if let Some(mut old) = old_session {
        let _ = old.child.kill().await;
    }

    // 2. Figure out the file paths from session metadata or remote context
    let ctx = remote.ok_or("Reconnect tail is only supported for remote sessions")?;
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == ctx.profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
    };

    let output_file = format!("{}/.operon-{}.jsonl", ctx.remote_path, session_id);
    let done_file = format!("{}/.operon-{}.done", ctx.remote_path, session_id);
    let shell = claude_shell()?;

    // 3. Build a fresh SSH tail command with tighter keepalives
    let mut ssh_tail_args = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
        profile.user, profile.host, profile.port
    );
    if let Some(ctrl_sock) = super::ssh::live_control_socket(&profile) {
        ssh_tail_args.push_str(&format!(
            " -o \"ControlPath={}\"",
            ctrl_sock.to_string_lossy()
        ));
    }
    if let Some(key) = &profile.key_file {
        // Single-quote the key path: it runs through `bash -l -c`, and a
        // Windows path (C:\Users\...) would otherwise have its backslashes
        // eaten as shell escapes, breaking key auth.
        ssh_tail_args.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
    }

    // Tail script: cat existing content, then tail -f for new lines.
    // If the done file already exists the session finished while we were stalled — just cat.
    // Uses stdbuf -oL to force line-buffered output and prevent SSH pipe buffering.
    // Hardened against orphaning: trap HUP/PIPE, self-destruct once PPID==1, 12h cap.
    // Wait up to 5 min for the file before declaring it missing — covers NFS
    // attribute-cache lag and slow startup. Heartbeats during the wait keep the
    // frontend's auto-reconnect watchdog quiet so it doesn't kill THIS tail and
    // recurse, producing duplicate "file not found" errors.
    let tail_script = format!(
        "trap '' HUP PIPE; \
         printf '{{\"type\":\"heartbeat\"}}\\n'; \
         if [ -f '{donef}' ]; then cat '{out}'; exit 0; fi; \
         i=0; while [ ! -f '{out}' ] && [ \"$i\" -lt 1500 ]; do sleep 0.2; i=$((i+1)); [ $((i % 50)) -eq 0 ] && printf '{{\"type\":\"heartbeat\"}}\\n'; done; \
         if [ ! -f '{out}' ]; then echo '{{\"type\":\"error\",\"error\":{{\"message\":\"Output file did not appear after 5 minutes — the remote command may have failed to start, or the file was cleaned up. Check the terminal for errors.\"}}}}'; exit 1; fi; \
         if command -v stdbuf >/dev/null 2>&1; then stdbuf -oL cat '{out}'; stdbuf -oL tail -f -n +$(($(wc -l < '{out}' | tr -d ' ') + 1)) '{out}' & TAIL_PID=$!; else cat '{out}'; tail -f -n +$(($(wc -l < '{out}' | tr -d ' ') + 1)) '{out}' & TAIL_PID=$!; fi; \
         ( _warned=0; while [ ! -f '{donef}' ]; do sleep 30; _pp=$(ps -o ppid= -p ${{BASHPID:-$$}} 2>/dev/null | tr -d ' '); [ \"$_pp\" = \"1\" ] && exit 0; printf '{{\"type\":\"heartbeat\"}}\\n'; if [ -f '{out}' ]; then _now=$(date +%s); _mt=$(stat -c %Y '{out}' 2>/dev/null || stat -f %m '{out}' 2>/dev/null || echo 0); if [ \"$_mt\" -gt 0 ]; then _age=$((_now - _mt)); if [ \"$_age\" -gt 300 ] && [ \"$_warned\" = \"0\" ]; then printf '{{\"type\":\"stall_warning\",\"message\":\"No output for over 5 minutes. This is normal for a long tool call, compile, download or queued job — the agent is still being watched. If the compute node was preempted or OOM-killed the session will end on its own.\"}}\\n'; _warned=1; fi; fi; fi; done ) & HB_PID=$!; \
         _n=0; _capped=0; while [ ! -f '{donef}' ]; do sleep 0.5; _n=$((_n+1)); [ \"$_n\" -ge 86400 ] && {{ _capped=1; break; }}; _pp=$(ps -o ppid= -p $$ 2>/dev/null | tr -d ' '); [ \"$_pp\" = \"1\" ] && {{ kill $TAIL_PID $HB_PID 2>/dev/null; exit 0; }}; done; \
         sleep 0.5; kill $TAIL_PID $HB_PID 2>/dev/null; wait $TAIL_PID $HB_PID 2>/dev/null; \
         [ \"$_capped\" = \"1\" ] && exit 0; \
         cat '{out}'",
        donef = done_file, out = output_file,
    );
    let b64_tail = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());
    ssh_tail_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64_tail));

    let mut tail_cmd = AsyncCommand::new(&shell);
    tail_cmd.arg("-l").arg("-c").arg(&ssh_tail_args);
    tail_cmd.stdout(std::process::Stdio::piped());
    tail_cmd.stderr(std::process::Stdio::piped());
    hide_window_async(&mut tail_cmd);

    let mut child = tail_cmd
        .spawn()
        .map_err(|e| format!("Failed to reconnect tail: {}", e))?;
    let stdout = child.stdout.take().ok_or("Failed to capture tail stdout")?;

    // 4. Store the new tail process as the session's child
    state
        .sessions
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id.clone(), ClaudeSession { child });

    // 5. Stream output to frontend — the dedup logic in the frontend handles
    //    re-sent lines gracefully (same message ID = replace, not duplicate).
    let app_handle = app.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let _ = app_handle.emit(
                &format!("claude-event-{}", sid),
                serde_json::json!({ "line": line }),
            );
        }
        let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
    });

    Ok(())
}

/// Rename a session (update its human-readable name).
#[tauri::command]
pub async fn rename_session(session_id: String, name: String) -> Result<(), String> {
    if let Some(mut meta) = load_session_from_disk(&session_id).map_err(|e| e.to_string())? {
        meta.name = Some(name);
        save_session_to_disk(&meta)?;
        Ok(())
    } else {
        Err(format!("Session {} not found", session_id))
    }
}

/// Delete a session's metadata and optionally its output files.
#[tauri::command]
pub async fn delete_session(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    session_id: String,
    remote: Option<RemoteContext>,
    delete_output: Option<bool>,
) -> Result<(), String> {
    // Load the metadata BEFORE deleting it — the output paths are derived from it.
    // This used to run after `remove_file`, so `load_session_from_disk` always
    // returned `None` and the whole `delete_output` block below was dead code: the
    // transcripts it promised to remove were never touched. That matters more now
    // that the remote tail no longer deletes them at the end of each turn — this is
    // the path that actually reaps them.
    let meta = load_session_from_disk(&session_id).ok().flatten();

    // Delete metadata file
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", session_id));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete session: {}", e))?;
    }

    // Optionally delete output files
    if delete_output.unwrap_or(false) {
        if let Some(meta) = meta {
            let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
            let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
            let done_file = format!("{}/.operon-{}.done", base_path, session_id);

            if let Some(ctx) = remote {
                let profile = {
                    let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                    profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
                };
                if let Some(profile) = profile {
                    let rm_cmd = format!(
                        "rm -f '{}' '{}'",
                        output_file.replace('\'', "'\\''"),
                        done_file.replace('\'', "'\\''"),
                    );
                    let _ = super::ssh::ssh_exec(&profile, &rm_cmd);
                }
            } else {
                let _ = std::fs::remove_file(&output_file);
                let _ = std::fs::remove_file(&done_file);
            }
        }
    }

    Ok(())
}

// ── "Stop everything" — scan & tear down Operon's remote footprint ──────────
//
// Powers the red stop-sign button. Two actions sit behind it:
//   • "Disconnect (keep remote session)"  → existing `stop_control_master` + UI reset.
//   • "End session & clean up everything"  → `scan_remote_footprint` → confirm →
//                                            `teardown_remote_footprint`.
// It kills what Operon spawned (local SSH children, remote log-streamers, the
// `operon-*` tmux session, scratch files) and — only for job ids the user ticks
// in the dialog — releases an *interactive* allocation with `scancel`.
//
// Why interactive allocations need their own explicit step: killing the tmux
// session used to end the allocation as a side effect, because the pane process
// WAS the `srun --pty` that owned it. Since the reuse-before-acquire feature the
// pane may instead hold `srun --jobid=N --overlap --pty`, which is a job *step*
// on an allocation someone else's process owns. Killing that pane releases
// nothing, and the next connect's reuse snippet re-attaches to the same job —
// "I ended the session but it picked the node right back up".
//
// SAFETY: `scancel` is *only ever* reachable for a job the remote re-verifies as
// `BatchFlag=0` (salloc / `srun --pty`) and owned by the connecting user. A batch
// job is never cancellable through this path, whatever the frontend sends.

/// A running *interactive* SLURM allocation (`BatchFlag=0`) belonging to the user.
#[derive(Debug, Serialize)]
pub struct InteractiveJob {
    pub id: String,
    pub name: String,
    pub nodes: String,
    /// Elapsed run time (`squeue %M`).
    pub time: String,
    /// Node list (`squeue %R`).
    pub nodelist: String,
    /// How Operon tied this allocation to its own tmux pane:
    /// * `"pane"` — a `--jobid=<id>` was found on an `operon-*` pane process. Certain.
    /// * `"sole"` — the only running interactive allocation while an `srun`/`salloc`
    ///   is live in an `operon-*` pane. Near-certain.
    /// * `"none"` — unattributed. Listed for the user, never pre-selected.
    pub attribution: String,
}

/// What Operon has left running / lying around on a remote host.
#[derive(Debug, Serialize)]
pub struct RemoteFootprint {
    /// `ps`-style lines for leftover Operon helper processes (log-streamers, tails).
    pub helpers: Vec<String>,
    /// tmux sessions whose name starts with `operon` (e.g. `operon-main`).
    pub tmux_sessions: Vec<String>,
    /// true if an `srun`/`salloc`/`sbatch` is running inside one of those tmux panes —
    /// i.e. killing the tmux session *might* release a SLURM allocation.
    pub slurm_in_pane: bool,
    /// `squeue` lines for the user's currently-running jobs (context for the warning).
    pub running_jobs: Vec<String>,
    /// Running interactive allocations the user can choose to release.
    pub interactive_jobs: Vec<InteractiveJob>,
    /// Set when the remote scan could not be completed. An empty footprint with
    /// `scan_error: None` means "nothing is running"; with `Some(..)` it means
    /// "we don't know" — the UI must not present the two as the same thing.
    pub scan_error: Option<String>,
}

/// A SLURM job id is digits plus the array (`_`), heterogeneous (`+`) and step
/// (`.`) separators — nothing else. Everything that reaches a remote `scancel`
/// is filtered through this, so a malformed or hostile id from the frontend can
/// never become shell syntax.
pub(crate) fn is_valid_job_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id.starts_with(|c: char| c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '+' || c == '.')
}

/// The remote-side guard for one `scancel`, emitting a one-word verdict.
///
/// The checks are re-run *on the server, immediately before the cancel* rather
/// than trusted from the scan, so a job that turned into something else (or was
/// never ours) between the dialog opening and the button being pressed is still
/// refused. `id` must have passed [`is_valid_job_id`] — it is interpolated bare.
///
/// Verdicts: `ok` cancelled · `fail` scancel errored · `batch` refused, not an
/// interactive allocation · `gone` not in the user's running queue any more.
///
/// The `BatchFlag` read is deliberately paranoid: tokenize on spaces, keep only
/// whole `BatchFlag=[01]` tokens, and take the FIRST one. Whole-token matching
/// stops a job whose *name* contains `BatchFlag=0` from passing, and taking the
/// first stops a later field that echoes user text (`Command=`, `StdOut=`) from
/// overriding the real flag. Anything unparseable falls through to `batch`,
/// i.e. refuse.
pub(crate) fn cancel_job_snippet(id: &str) -> String {
    debug_assert!(is_valid_job_id(id));
    format!(
        "if [ -n \"$(squeue -h -j {id} -u \"$_u\" -o '%i' 2>/dev/null)\" ]; then \
           if [ \"$(scontrol show job {id} 2>/dev/null | tr ' ' '\\n' | grep -x 'BatchFlag=[01]' | head -1)\" = 'BatchFlag=0' ]; then \
             scancel {id} >/dev/null 2>&1 && echo 'ok {id}' || echo 'fail {id}'; \
           else echo 'batch {id}'; fi; \
         else echo 'gone {id}'; fi\n",
        id = id
    )
}

/// Turn the `__INTERACTIVE__` lines (`id|name|nodes|time|nodelist`) into typed
/// jobs, tagging each with how confidently it belongs to Operon's tmux pane.
///
/// `pane_job_ids` are the ids scraped off `--jobid=` on an `operon-*` pane
/// process — a positive identification. When there is no such id but an
/// `srun`/`salloc` *is* live in an Operon pane and the user has exactly one
/// interactive allocation, that allocation is necessarily the one the pane is
/// sitting in. Anything else is left unattributed so the UI never pre-selects a
/// job Operon cannot account for.
pub(crate) fn attribute_interactive_jobs(
    raw: &[String],
    pane_job_ids: &[String],
    slurm_in_pane: bool,
) -> Vec<InteractiveJob> {
    let mut jobs: Vec<InteractiveJob> = raw
        .iter()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            let id = f.first().copied().unwrap_or_default();
            if !is_valid_job_id(id) {
                return None;
            }
            Some(InteractiveJob {
                id: id.to_string(),
                name: f.get(1).copied().unwrap_or_default().to_string(),
                nodes: f.get(2).copied().unwrap_or_default().to_string(),
                time: f.get(3).copied().unwrap_or_default().to_string(),
                nodelist: f.get(4).copied().unwrap_or_default().to_string(),
                attribution: "none".to_string(),
            })
        })
        .collect();
    jobs.dedup_by(|a, b| a.id == b.id);

    let mut matched_pane = false;
    for job in jobs.iter_mut() {
        if pane_job_ids.iter().any(|p| p.trim() == job.id) {
            job.attribution = "pane".to_string();
            matched_pane = true;
        }
    }
    if !matched_pane && slurm_in_pane && jobs.len() == 1 {
        jobs[0].attribution = "sole".to_string();
    }
    jobs
}

#[tauri::command]
pub async fn scan_remote_footprint(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<RemoteFootprint, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // `H` is built via `printf` so this script's own command line does not contain
    // the literal token "operon-<hex>" — otherwise `pgrep` would list the shell
    // running this very script.
    let script = r#"
H=$(printf 'operon')
U=$(id -un 2>/dev/null || echo "$USER")
T=""; command -v timeout >/dev/null 2>&1 && T="timeout 10"
PANES=$(tmux list-panes -aF '#{session_name} #{pane_pid}' 2>/dev/null | awk -v h="$H" '$1 ~ ("^" h) {print $2}')
echo '__HELPERS__'
pgrep -af "${H}-[0-9a-f]" 2>/dev/null | grep -v -e 'pgrep ' || true
echo '__TMUX__'
tmux ls -F '#{session_name}' 2>/dev/null | grep -E "^${H}" || true
echo '__PANEPROCS__'
for _p in $PANES; do
  ps -p "$_p" -o args= 2>/dev/null
  for _c in $(pgrep -P "$_p" 2>/dev/null); do
    ps -p "$_c" -o args= 2>/dev/null
    for _g in $(pgrep -P "$_c" 2>/dev/null); do ps -p "$_g" -o args= 2>/dev/null; done
  done
done
echo '__PANEJOBS__'
for _p in $PANES; do
  for _q in "$_p" $(pgrep -P "$_p" 2>/dev/null); do
    ps -p "$_q" -o args= 2>/dev/null | awk '{for(i=1;i<=NF;i++){if($i ~ /^--jobid=/){sub(/^--jobid=/,"",$i); print $i} else if($i=="--jobid" && i<NF){print $(i+1)}}}'
  done
done
echo '__SLURM__'
squeue -u "$U" -h -o '%i %j %T %D %m %R' 2>/dev/null || true
echo '__INTERACTIVE__'
for _j in $($T squeue -h -u "$U" -t R -o '%i' 2>/dev/null | grep -v '_' | head -20); do
  $T scontrol show job "$_j" 2>/dev/null | tr ' ' '\n' | grep -qx 'BatchFlag=0' && \
    $T squeue -h -j "$_j" -o '%i|%j|%D|%M|%R' 2>/dev/null
done
echo '__END__'
"#;
    // A failed scan must NOT look like a clean server. `.unwrap_or_default()`
    // used to turn an SSH error into an empty footprint, which the dialog then
    // rendered as "No Operon tmux session found" and the teardown reported as a
    // successful kill.
    let (out, mut scan_error) = match super::ssh::ssh_exec(&profile, script) {
        Ok(o) => (o, None),
        Err(e) => (String::new(), Some(e)),
    };
    // The script ends with a sentinel, so a connection that dropped mid-stream
    // (or a shell that died on the way) is detectable even though `ssh_exec`
    // returned `Ok`.
    if scan_error.is_none() && !out.lines().any(|l| l.trim() == "__END__") {
        scan_error = Some("the remote scan did not run to completion".to_string());
    }

    let mut section = "";
    let mut helpers = Vec::new();
    let mut tmux_sessions = Vec::new();
    let mut pane_procs: Vec<String> = Vec::new();
    let mut pane_job_ids: Vec<String> = Vec::new();
    let mut interactive_raw: Vec<String> = Vec::new();
    let mut running_jobs = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        match t {
            "__HELPERS__" => {
                section = "h";
                continue;
            }
            "__TMUX__" => {
                section = "t";
                continue;
            }
            "__PANEPROCS__" => {
                section = "p";
                continue;
            }
            "__PANEJOBS__" => {
                section = "j";
                continue;
            }
            "__SLURM__" => {
                section = "s";
                continue;
            }
            "__INTERACTIVE__" => {
                section = "i";
                continue;
            }
            "__END__" => {
                section = "";
                continue;
            }
            _ => {}
        }
        if t.is_empty() {
            continue;
        }
        match section {
            "h" => helpers.push(t.to_string()),
            "t" => tmux_sessions.push(t.to_string()),
            "p" => pane_procs.push(t.to_string()),
            "j" => pane_job_ids.push(t.to_string()),
            "s" => running_jobs.push(t.to_string()),
            "i" => interactive_raw.push(t.to_string()),
            _ => {}
        }
    }
    let slurm_in_pane = pane_procs.iter().any(|p| {
        let p = p.trim_start();
        p.starts_with("srun")
            || p.starts_with("salloc")
            || p.starts_with("sbatch")
            || p.contains(" srun ")
            || p.contains(" salloc ")
            || p.contains(" sbatch ")
    });

    let interactive_jobs =
        attribute_interactive_jobs(&interactive_raw, &pane_job_ids, slurm_in_pane);

    Ok(RemoteFootprint {
        helpers,
        tmux_sessions,
        slurm_in_pane,
        running_jobs,
        interactive_jobs,
        scan_error,
    })
}

#[tauri::command]
pub async fn teardown_remote_footprint(
    claude_state: tauri::State<'_, ClaudeManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
    kill_tmux: bool,
    // `cancel_job_ids`: interactive allocations the user explicitly ticked in the
    // confirm dialog. Validated here and re-verified on the remote before any
    // `scancel`.
    cancel_job_ids: Vec<String>,
) -> Result<String, String> {
    // 1. Kill the local SSH child processes Operon spawned for sessions on this
    //    profile (log-streamers, reconnect tails, headless claude). With the
    //    hardened remote tail script, killing the local ssh makes the remote side
    //    self-destruct within ~0.5s.
    let to_kill: Vec<tokio::process::Child> = {
        let mut sessions = claude_state.sessions.lock().map_err(|e| e.to_string())?;
        let ids: Vec<String> = sessions
            .keys()
            .filter(|id| {
                matches!(
                    load_session_from_disk(id),
                    Ok(Some(ref m)) if m.profile_id.as_deref() == Some(profile_id.as_str())
                )
            })
            .cloned()
            .collect();
        ids.into_iter()
            .filter_map(|id| sessions.remove(&id).map(|s| s.child))
            .collect()
    };
    let n_local = to_kill.len();
    for mut c in to_kill {
        let _ = c.kill().await;
    }

    // 2. Build the remote cleanup script.
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // `$_self` excludes the shell running this script, which `pgrep` would otherwise
    // match (its command line contains the pattern strings).
    let mut script = String::from(
        "_self=$$\n\
         for _p in $(pgrep -f 'tail.*-f.*\\.operon-' 2>/dev/null); do [ \"$_p\" = \"$_self\" ] || kill \"$_p\" 2>/dev/null; done\n\
         for _p in $(pgrep -f 'base64 -d.*bash' 2>/dev/null); do [ \"$_p\" = \"$_self\" ] || { pkill -P \"$_p\" 2>/dev/null; kill \"$_p\" 2>/dev/null; }; done\n",
    );

    // Remove leftover .operon-* scratch files for this profile's sessions.
    let mut rm_targets: Vec<String> = Vec::new();
    for meta in load_all_sessions_from_disk() {
        if meta.profile_id.as_deref() != Some(profile_id.as_str()) {
            continue;
        }
        let base = meta
            .remote_path
            .as_deref()
            .unwrap_or(meta.project_path.as_str())
            .replace('\'', "'\\''");
        let sid = &meta.session_id;
        for ext in ["jsonl", "done", "host"] {
            rm_targets.push(format!("'{}/.operon-{}.{}'", base, sid, ext));
        }
        rm_targets.push(format!("'{}/.operon-run-{}.sh'", base, sid));
        rm_targets.push(format!("'{}/.operon-report-prompt-{}.txt'", base, sid));
    }
    for chunk in rm_targets.chunks(40) {
        script.push_str(&format!("rm -f {} 2>/dev/null\n", chunk.join(" ")));
    }

    if kill_tmux {
        script.push_str(
            "for _s in $(tmux ls -F '#{session_name}' 2>/dev/null | grep -E '^operon'); do tmux kill-session -t \"$_s\" 2>/dev/null; done\n",
        );
    }

    // 3. Release the interactive allocations the user ticked. Killing the tmux
    //    pane only ends an allocation the pane's `srun` actually owns — a reuse
    //    step (`srun --jobid=N --overlap`) leaves the job running and the next
    //    connect re-attaches to it. This is the step that closes that loop.
    //
    //    The remote re-verifies ownership and `BatchFlag=0` immediately before
    //    every `scancel`, so the frontend cannot talk this into cancelling a
    //    batch job or someone else's work no matter what it sends.
    let cancel_ids: Vec<String> = cancel_job_ids
        .into_iter()
        .filter(|id| is_valid_job_id(id))
        .collect();
    if !cancel_ids.is_empty() {
        script.push_str("_u=$(id -un 2>/dev/null || echo \"$USER\")\n");
        script.push_str("echo '__CANCEL__'\n");
        for id in &cancel_ids {
            script.push_str(&cancel_job_snippet(id));
        }
    }

    // 4. Report what is ACTUALLY left, not what we asked for. Every step above is
    //    best-effort on a remote we may not even have reached.
    script.push_str("echo '__VTMUX__'\n");
    script.push_str("tmux ls -F '#{session_name}' 2>/dev/null | grep -E '^operon' || true\n");
    script.push_str("echo '__VJOBS__'\n");
    script.push_str(
        "squeue -h -u \"$(id -un 2>/dev/null || echo \"$USER\")\" -t R -o '%i' 2>/dev/null || true\n",
    );
    script.push_str("echo '__VEND__'\n");

    let outcome = super::ssh::ssh_exec(&profile, &script);
    Ok(teardown_report(
        n_local,
        kill_tmux,
        &cancel_ids,
        outcome.as_deref(),
    ))
}

/// Build the human-readable teardown summary from the remote transcript.
///
/// Split out from `teardown_remote_footprint` so the reporting can be tested
/// without a cluster. The old code discarded the SSH result entirely and
/// unconditionally claimed "killed Operon tmux session(s)" — including when the
/// connection failed and nothing ran at all.
pub(crate) fn teardown_report(
    n_local: usize,
    kill_tmux: bool,
    cancel_ids: &[String],
    outcome: Result<&str, &String>,
) -> String {
    let mut lines = vec![format!("Stopped {} local Operon SSH process(es).", n_local)];

    let out = match outcome {
        Ok(o) => o,
        Err(e) => {
            lines.push(format!(
                "Could NOT reach the server — no remote cleanup ran: {e}"
            ));
            lines.push(
                "Reconnect and try again; the tmux session and any allocation are still up."
                    .to_string(),
            );
            return lines.join("\n");
        }
    };

    // Section the transcript.
    let mut section = "";
    let mut cancel_results: Vec<(&str, &str)> = Vec::new();
    let mut tmux_left: Vec<&str> = Vec::new();
    let mut jobs_left: Vec<&str> = Vec::new();
    let mut saw_end = false;
    for line in out.lines() {
        let t = line.trim();
        match t {
            "__CANCEL__" => {
                section = "c";
                continue;
            }
            "__VTMUX__" => {
                section = "t";
                continue;
            }
            "__VJOBS__" => {
                section = "j";
                continue;
            }
            "__VEND__" => {
                saw_end = true;
                section = "";
                continue;
            }
            _ => {}
        }
        if t.is_empty() {
            continue;
        }
        match section {
            "c" => {
                if let Some((verdict, id)) = t.split_once(' ') {
                    cancel_results.push((verdict, id.trim()));
                }
            }
            "t" => tmux_left.push(t),
            "j" => jobs_left.push(t),
            _ => {}
        }
    }

    if !saw_end {
        lines.push(
            "The remote cleanup script did not run to completion — treat everything below as unverified."
                .to_string(),
        );
    }

    lines.push("Cleaned up remote log-streamers and scratch files.".to_string());

    if kill_tmux {
        if tmux_left.is_empty() {
            lines.push("Killed Operon's tmux session(s).".to_string());
        } else {
            lines.push(format!(
                "WARNING: tmux session(s) still present after the kill: {}",
                tmux_left.join(", ")
            ));
        }
    } else {
        lines.push("Left Operon's tmux session running (not selected).".to_string());
    }

    for id in cancel_ids {
        let verdict = cancel_results
            .iter()
            .find(|(_, j)| j == id)
            .map(|(v, _)| *v)
            .unwrap_or("unknown");
        let still_running = jobs_left
            .iter()
            .any(|j| j.split_whitespace().next() == Some(id.as_str()));
        let line = match verdict {
            _ if still_running => format!("WARNING: job {} is STILL RUNNING after scancel.", id),
            "ok" => format!("Released interactive allocation {}.", id),
            "gone" => format!("Allocation {} had already ended.", id),
            "batch" => format!(
                "Refused to cancel {}: it is a batch job, not an interactive allocation.",
                id
            ),
            "fail" => format!("scancel {} failed on the server.", id),
            _ => format!("Could not confirm what happened to job {}.", id),
        };
        lines.push(line);
    }

    lines.join("\n")
}

#[cfg(test)]
mod teardown_tests {
    use super::*;

    fn raw(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    // ── job-id validation: the only thing standing between the frontend and a
    //    remote shell ──────────────────────────────────────────────────────────

    #[test]
    fn accepts_real_slurm_job_ids() {
        for id in ["54585562", "54621366_103", "12345+0", "999.4"] {
            assert!(is_valid_job_id(id), "{id} should be valid");
        }
    }

    #[test]
    fn a_job_id_can_never_carry_shell_syntax() {
        for id in [
            "",
            "123; rm -rf ~",
            "123 456",
            "$(id)",
            "`id`",
            "123|cat",
            "123'",
            "-u",
            "abc",
            "'",
        ] {
            assert!(!is_valid_job_id(id), "{id:?} must be rejected");
        }
    }

    #[test]
    fn a_job_id_cannot_be_unbounded() {
        assert!(!is_valid_job_id(&"1".repeat(33)));
    }

    // ── attribution: which allocation the dialog is allowed to pre-tick ───────

    #[test]
    fn a_jobid_on_an_operon_pane_is_a_certain_match() {
        let jobs = attribute_interactive_jobs(
            &raw(&[
                "54585562|bash|4|16:37:54|hpc3-21-32",
                "77777|salloc|1|0:10|hpc3-14-01",
            ]),
            &raw(&["54585562"]),
            true,
        );
        assert_eq!(jobs[0].attribution, "pane");
        // A second allocation the pane is NOT in stays unattributed, so the UI
        // never pre-selects someone's unrelated interactive work.
        assert_eq!(jobs[1].attribution, "none");
    }

    #[test]
    fn the_sole_allocation_under_a_live_srun_is_ours() {
        let jobs =
            attribute_interactive_jobs(&raw(&["54585562|bash|4|16:37:54|hpc3-21-32"]), &[], true);
        assert_eq!(jobs[0].attribution, "sole");
    }

    #[test]
    fn nothing_is_attributed_without_an_srun_in_an_operon_pane() {
        // No Operon pane holds a SLURM process — the user's interactive node was
        // started somewhere else entirely and must not be pre-ticked.
        let jobs =
            attribute_interactive_jobs(&raw(&["54585562|bash|4|16:37:54|hpc3-21-32"]), &[], false);
        assert_eq!(jobs[0].attribution, "none");
    }

    #[test]
    fn ambiguity_is_never_resolved_by_guessing() {
        // Two interactive allocations, no --jobid to disambiguate: guessing would
        // mean cancelling a coin-flip. Both stay unattributed.
        let jobs = attribute_interactive_jobs(
            &raw(&["111|bash|1|1:00|n1", "222|bash|1|2:00|n2"]),
            &[],
            true,
        );
        assert!(jobs.iter().all(|j| j.attribution == "none"));
    }

    #[test]
    fn squeue_column_padding_is_stripped() {
        let jobs = attribute_interactive_jobs(
            &raw(&["54585562  |bash      |4  |16:37:54|hpc3-21-32 "]),
            &raw(&["54585562 "]),
            true,
        );
        assert_eq!(jobs[0].id, "54585562");
        assert_eq!(jobs[0].name, "bash");
        assert_eq!(jobs[0].nodelist, "hpc3-21-32");
        assert_eq!(jobs[0].attribution, "pane");
    }

    #[test]
    fn garbage_squeue_lines_are_dropped_not_passed_through() {
        let jobs = attribute_interactive_jobs(
            &raw(&[
                "",
                "slurm_load_jobs error: Unable to contact slurm controller",
                "111|bash|1|1:00|n1",
            ]),
            &[],
            false,
        );
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "111");
    }

    // ── the scancel guard (branch behaviour verified against stub squeue /
    //    scontrol / scancel; these lock the shape the harness exercised) ───────

    #[test]
    fn the_cancel_guard_re_verifies_on_the_server_before_cancelling() {
        let s = cancel_job_snippet("54585562");
        // Ownership: the job must still be in THIS user's running queue.
        assert!(s.contains("squeue -h -j 54585562 -u \"$_u\""), "{s}");
        // Interactive-only: whole-token BatchFlag match, first occurrence wins.
        assert!(s.contains("grep -x 'BatchFlag=[01]' | head -1"), "{s}");
        assert!(s.contains("= 'BatchFlag=0'"), "{s}");
        // Every branch reports a verdict — silence is never a possible outcome.
        for verdict in [
            "ok 54585562",
            "fail 54585562",
            "batch 54585562",
            "gone 54585562",
        ] {
            assert!(s.contains(verdict), "missing verdict {verdict} in {s}");
        }
    }

    #[test]
    fn the_cancel_guard_refuses_by_default() {
        // The `else` of the BatchFlag comparison is `batch` (refuse), not scancel —
        // so unparseable/missing scontrol output can never cancel anything.
        let s = cancel_job_snippet("999");
        let scancel_at = s.find("scancel 999").unwrap();
        let refuse_at = s.find("echo 'batch 999'").unwrap();
        assert!(
            refuse_at > scancel_at,
            "refusal must be the else-branch: {s}"
        );
    }

    // ── reporting: the bug the user actually saw was Operon claiming success ──

    #[test]
    fn an_unreachable_server_is_never_reported_as_a_successful_kill() {
        let err = "ssh: connect to host hpc3 port 22: Operation timed out".to_string();
        let msg = teardown_report(2, true, &["54585562".into()], Err(&err));
        assert!(!msg.contains("Killed"), "{msg}");
        assert!(!msg.contains("Released"), "{msg}");
        assert!(msg.contains("Could NOT reach"), "{msg}");
        assert!(msg.contains("still up"), "{msg}");
    }

    #[test]
    fn a_surviving_tmux_session_is_reported_as_a_warning() {
        let out = "__VTMUX__\noperon-node\n__VJOBS__\n__VEND__\n";
        let msg = teardown_report(1, true, &[], Ok(out));
        assert!(msg.contains("WARNING"), "{msg}");
        assert!(msg.contains("operon-node"), "{msg}");
        assert!(!msg.contains("Killed Operon's tmux"), "{msg}");
    }

    #[test]
    fn a_clean_kill_is_reported_plainly() {
        let out = "__CANCEL__\nok 54585562\n__VTMUX__\n__VJOBS__\n__VEND__\n";
        let msg = teardown_report(1, true, &["54585562".into()], Ok(out));
        assert!(msg.contains("Killed Operon's tmux session(s)."), "{msg}");
        assert!(
            msg.contains("Released interactive allocation 54585562."),
            "{msg}"
        );
        assert!(!msg.contains("WARNING"), "{msg}");
    }

    #[test]
    fn a_job_still_in_squeue_after_scancel_beats_the_ok_verdict() {
        // scancel returned 0 but the allocation outlived it. The post-hoc squeue
        // is the authority — this is exactly the "Operon said it killed it" case.
        let out = "__CANCEL__\nok 54585562\n__VTMUX__\n__VJOBS__\n54585562\n__VEND__\n";
        let msg = teardown_report(1, true, &["54585562".into()], Ok(out));
        assert!(msg.contains("STILL RUNNING"), "{msg}");
        assert!(!msg.contains("Released interactive allocation"), "{msg}");
    }

    #[test]
    fn a_batch_job_refusal_is_surfaced_to_the_user() {
        let out = "__CANCEL__\nbatch 54621366\n__VTMUX__\n__VJOBS__\n__VEND__\n";
        let msg = teardown_report(1, true, &["54621366".into()], Ok(out));
        assert!(msg.contains("Refused to cancel 54621366"), "{msg}");
    }

    #[test]
    fn a_truncated_transcript_marks_everything_unverified() {
        // Connection dropped mid-script: no __VEND__ sentinel.
        let msg = teardown_report(1, true, &[], Ok("__VTMUX__\n"));
        assert!(msg.contains("did not run to completion"), "{msg}");
    }

    #[test]
    fn declining_the_tmux_kill_says_so_rather_than_claiming_it() {
        let msg = teardown_report(
            0,
            false,
            &[],
            Ok("__VTMUX__\noperon-node\n__VJOBS__\n__VEND__\n"),
        );
        assert!(msg.contains("Left Operon's tmux session running"), "{msg}");
        assert!(!msg.contains("WARNING"), "{msg}");
    }
}

/// Drives the real `GUARD_HOOK_SCRIPT` with a stub reviewer.
///
/// The hook is a shell script embedded in a Rust string that only ever runs on
/// a compute node, which is how it drifted: for 93 recorded production reviews
/// it managed exactly one true catch, because it kept reviewing the wrong file
/// (a `conda.sh`, a log, a job-id list), skipping `cd X && sbatch` entirely, and
/// reporting `high` findings as "clean". Nothing in CI executed a single line of
/// it. These tests do, against the same constant that ships.
#[cfg(all(test, unix))]
mod guard_hook_tests {
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::Command;

    fn have(bin: &str) -> bool {
        Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {bin}"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    struct Env {
        root: PathBuf,
        home: PathBuf,
        work: PathBuf,
        /// A directory that is NOT the submit dir — the hook runs from here, as
        /// it does in production, so `cd`-relative resolution is actually tested.
        elsewhere: PathBuf,
        hook: PathBuf,
        mock_out: PathBuf,
        events: PathBuf,
    }

    fn write(p: &PathBuf, s: &str) {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::File::create(p)
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }

    fn chmod_x(p: &PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    impl Env {
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("operon-guard-{}-{}", std::process::id(), tag));
            let _ = std::fs::remove_dir_all(&root);
            let home = root.join("home");
            let work = root.join("work");
            let elsewhere = root.join("elsewhere");
            std::fs::create_dir_all(&work).unwrap();
            std::fs::create_dir_all(&elsewhere).unwrap();
            let hook = home.join(".operon/guard/operon-guard.sh");
            write(&hook, super::GUARD_HOOK_SCRIPT);
            chmod_x(&hook);

            // Stub reviewer: ignores the prompt, replays a canned CLI envelope.
            let mock = root.join("mockclaude");
            let mock_out = root.join("mock_out.json");
            write(&mock, "#!/bin/sh\ncat >/dev/null\ncat \"$MOCK_OUT\"\n");
            chmod_x(&mock);

            let events = home.join(".operon/reviews/test.jsonl");
            write(
                &home.join(".operon/guard/operon-review.env"),
                &format!(
                    "OPERON_REVIEW_ENABLED=1\nOPERON_REVIEW_MODEL='claude-sonnet-5'\nOPERON_REVIEW_EFFORT=''\nOPERON_REVIEW_EVENTS=\"{}\"\nOPERON_REVIEW_CLAUDE='{}'\n",
                    events.display(),
                    mock.display()
                ),
            );

            // A real sbatch script, plus the decoy files that fooled the old rule.
            write(
                &work.join("job.sh"),
                "#!/bin/bash\n#SBATCH --time=10:00\npython train.py --out /tmp/m.pt\n",
            );
            write(
                &work.join("conda.sh"),
                "#!/bin/sh\nexport PATH=/opt/conda/bin:$PATH\n",
            );
            write(&work.join(".mast_rerun_jobids"), "12345\n");
            write(&work.join("logs/out.txt"), "previous run log\n");

            Env {
                root,
                home,
                work,
                elsewhere,
                hook,
                mock_out,
                events,
            }
        }

        /// Run the hook on `cmd` with the reviewer returning `findings`.
        /// Returns (exit code, recorded outcome, reviewed script basename).
        fn run(&self, cmd: &str, findings: &str) -> (i32, String, String) {
            let _ = std::fs::remove_dir_all(self.home.join(".operon/guard/.review-seen"));
            let _ = std::fs::remove_file(&self.events);
            write(
                &self.mock_out,
                &serde_json::json!({ "type": "result", "result": findings }).to_string(),
            );
            let payload = serde_json::json!({ "tool_input": { "command": cmd } }).to_string();
            let script = format!(
                "printf '%s' {} | bash {}",
                shell_quote(&payload),
                shell_quote(&self.hook.to_string_lossy())
            );
            let out = Command::new("sh")
                .arg("-c")
                .arg(&script)
                .current_dir(&self.elsewhere)
                .env("HOME", &self.home)
                .env("MOCK_OUT", &self.mock_out)
                .output()
                .expect("run hook");
            let code = out.status.code().unwrap_or(-1);
            let ev = std::fs::read_to_string(&self.events).unwrap_or_default();
            let last = ev.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("");
            let v: serde_json::Value =
                serde_json::from_str(last).unwrap_or(serde_json::Value::Null);
            (
                code,
                v.get("outcome")
                    .and_then(|x| x.as_str())
                    .unwrap_or("none")
                    .to_string(),
                v.get("script")
                    .and_then(|x| x.as_str())
                    .unwrap_or("none")
                    .to_string(),
            )
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn shell_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    const BLOCKING: &str = r#"{"findings":[{"severity":"blocking","line":3,"title":"tmp output","why_wrong":"w","fix":"f"}]}"#;
    const HIGH: &str = r#"{"findings":[{"severity":"high","line":3,"title":"tmp output","why_wrong":"w","fix":"f"}]}"#;
    const EMPTY: &str = r#"{"findings":[]}"#;

    fn skip() -> bool {
        if have("bash") && have("python3") {
            return false;
        }
        eprintln!("skipping guard hook tests: bash/python3 unavailable");
        true
    }

    // ── it must review the script sbatch actually runs ───────────────────────

    #[test]
    fn reviews_the_sbatch_script_not_the_first_file_on_the_line() {
        if skip() {
            return;
        }
        let e = Env::new("target");
        let w = e.work.display();
        // Every one of these picked the WRONG file (or nothing) before the fix;
        // each shape is taken from the reviewer's own production event log.
        for cmd in [
            format!("cd {w} && sbatch job.sh"),
            format!("cd {w}; sbatch job.sh"),
            format!("cd \"{w}\" && sbatch job.sh"),
            format!("sbatch --chdir={w} job.sh"),
            format!("sbatch -D {w} job.sh"),
            format!("sbatch {w}/job.sh"),
            format!("cd {w} && source conda.sh && sbatch job.sh"),
            format!("cd {w} && sbatch -o logs/out.txt job.sh"),
            format!("cd {w} && sbatch --dependency=afterok:1 .mast_rerun_jobids job.sh"),
        ] {
            let (code, outcome, script) = e.run(&cmd, BLOCKING);
            assert_eq!(script, "job.sh", "reviewed the wrong file for: {cmd}");
            assert_eq!(outcome, "blocked", "for: {cmd}");
            assert_eq!(code, 2, "a blocking finding must block: {cmd}");
        }
    }

    // ── severity must not be silently downgraded to "clean" ──────────────────

    #[test]
    fn a_high_finding_is_surfaced_not_reported_as_clean() {
        if skip() {
            return;
        }
        let e = Env::new("sev");
        let cmd = format!("cd {} && sbatch job.sh", e.work.display());
        let (code, outcome, _) = e.run(&cmd, HIGH);
        // Surfaced as its own outcome...
        assert_eq!(outcome, "warned", "a high finding must not read as clean");
        // ...but still does not stop the job: blocking is reserved for "blocking".
        assert_eq!(code, 0, "high must not block a real submission");
    }

    #[test]
    fn only_an_empty_review_counts_as_clean() {
        if skip() {
            return;
        }
        let e = Env::new("clean");
        let cmd = format!("cd {} && sbatch job.sh", e.work.display());
        assert_eq!(e.run(&cmd, EMPTY), (0, "clean".into(), "job.sh".into()));
    }

    #[test]
    fn an_unparseable_review_is_unavailable_never_clean() {
        if skip() {
            return;
        }
        let e = Env::new("garbage");
        let cmd = format!("cd {} && sbatch job.sh", e.work.display());
        let (code, outcome, _) = e.run(&cmd, "not json at all");
        assert_eq!(outcome, "unavailable");
        assert_eq!(code, 0, "reviewer trouble must never block a real job");
    }

    // ── the block-exemption is one submit, not forever ───────────────────────

    #[test]
    fn a_block_is_overridden_by_an_immediate_resubmit_only() {
        if skip() {
            return;
        }
        let e = Env::new("once");
        let cmd = format!("cd {} && sbatch job.sh", e.work.display());
        // `run` clears the marker, so drive the hook directly for submit #2.
        let (first, _, _) = e.run(&cmd, BLOCKING);
        assert_eq!(first, 2, "first submit must block");
        write(
            &e.mock_out,
            &serde_json::json!({"type":"result","result":BLOCKING}).to_string(),
        );
        let payload = serde_json::json!({ "tool_input": { "command": cmd } }).to_string();
        let out = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "printf '%s' {} | bash {}",
                shell_quote(&payload),
                shell_quote(&e.hook.to_string_lossy())
            ))
            .current_dir(&e.elsewhere)
            .env("HOME", &e.home)
            .env("MOCK_OUT", &e.mock_out)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "identical resubmit must pass");
        // And the marker must be session-scoped: setup wipes it next run.
        assert!(
            super::remote_guard_setup_block(&super::ReviewGuardCfg {
                enabled: true,
                model: "claude-sonnet-5".into(),
                effort: "low".into(),
                claude: None,
                events_path: "$HOME/.operon/reviews/x.jsonl".into(),
            })
            .contains(".review-seen"),
            "session setup must clear the exemption cache"
        );
    }

    // ── the hook's original job must still work ──────────────────────────────

    #[test]
    fn the_deletion_guard_still_denies_and_still_lets_normal_work_through() {
        if skip() {
            return;
        }
        let e = Env::new("del");
        for bad in [
            "rm -rf /data/results",
            "python -c \"import shutil; shutil.rmtree('/x')\"",
            "find . -name '*.tmp' -delete",
            "git clean -fd",
            "rsync -a --delete src/ dst/",
        ] {
            assert_eq!(e.run(bad, EMPTY).0, 2, "must deny: {bad}");
        }
        for ok in ["ls -la", "cp a b", "squeue -u $USER"] {
            assert_eq!(e.run(ok, EMPTY).0, 0, "must allow: {ok}");
        }
    }
}

/// Invariants of the HPC terminal-mode file protocol.
///
/// The three files (`.jsonl` output, `.done` marker, run script) are the only
/// coordination between three processes on two machines: the agent in a tmux pane
/// on a compute node, the tail on a login node, and Operon locally. Nothing here
/// can be exercised without a cluster, so these lock the *contract* at the source
/// level — every one of them encodes a bug that shipped.
#[cfg(test)]
mod terminal_mode_protocol_tests {
    const SRC: &str = include_str!("claude.rs");

    /// Body of `claude.rs` minus this test module, so assertions about "the code"
    /// are not satisfied by the test module's own text.
    fn code() -> &'static str {
        // The needle is split so it does not appear literally in the region it
        // is searching for.
        let marker = concat!("mod terminal_mode_", "protocol_tests");
        match SRC.find(marker) {
            Some(i) => &SRC[..i],
            None => SRC,
        }
    }

    #[test]
    fn the_tail_never_creates_the_done_marker() {
        // The tail used to `touch` .done after 5 minutes of output silence, which
        // declared a live agent dead. A long Bash step, a compile, an NFS stall or
        // a queued job routinely exceeds that, and stream-json emits nothing
        // between tool boundaries — so this fired during normal work.
        assert!(
            !code().contains("touch '{donef}'"),
            "the tail is creating .done again — only the run script may do that"
        );
    }

    #[test]
    fn the_tail_never_deletes_the_transcript() {
        // It also `rm -f`'d the .jsonl, so the evidence of what the agent had done
        // went with it. Cleanup belongs to delete_session / teardown, which know
        // when the user is actually finished.
        assert!(
            !code().contains("rm -f '{out}' '{donef}'"),
            "the tail is deleting session output again"
        );
    }

    #[test]
    fn a_five_minute_silence_is_a_warning_not_a_death() {
        assert!(
            code().contains("stall_warning"),
            "the silence path should emit a non-fatal stall_warning"
        );
        assert!(
            !code().contains("Agent appears to have died"),
            "silence is being reported as death again"
        );
    }

    #[test]
    fn the_run_script_clears_a_stale_done_before_starting() {
        // A .done left by a killed tail made the next turn's tail see "already
        // finished" and exit instantly, so the new agent streamed nothing at all.
        // The run script owns .done, so it owns clearing it.
        let c = code();
        let start = c
            .find("cd '{}' && rm -f '{}' '{}';")
            .expect("run script no longer clears stale out/done before the agent starts");
        let claude_at = c[start..]
            .find("{} > '{}' 2>&1; echo $? > '{}'")
            .expect("run script shape changed");
        assert!(claude_at > 0, "the clear must precede the agent invocation");
    }

    #[test]
    fn stop_interrupts_the_pane_for_terminal_mode_sessions() {
        // Killing the tracked child only kills the login-node tail; the agent
        // itself lives in the user's tmux pane on the compute node.
        let c = code();
        let stop = c
            .find("pub async fn stop_claude_session")
            .expect("stop_claude_session missing");
        let body = &c[stop..];
        let end = body.find("\n#[tauri::command]").unwrap_or(body.len());
        let body = &body[..end];
        assert!(
            body.contains("write_terminal_bytes"),
            "Stop no longer delivers an interrupt to the terminal"
        );
        assert!(
            body.contains(r"\x03"),
            "Stop should send ETX (Ctrl-C) to the pane"
        );
        assert!(
            body.contains("use_terminal"),
            "the interrupt must be gated on terminal-mode sessions"
        );
    }
}

#[cfg(test)]
mod session_status_tests {
    use super::status_transition_allowed;

    #[test]
    fn stopped_is_terminal() {
        // The exact sequence Stop produces: the button writes "stopped", then the
        // child dies, then the done handler tries to write "completed".
        assert!(!status_transition_allowed("stopped", "completed"));
        assert!(!status_transition_allowed("stopped", "failed"));
        assert!(!status_transition_allowed("stopped", "running"));
    }

    #[test]
    fn every_other_transition_still_works() {
        for (from, to) in [
            ("running", "completed"),
            ("running", "failed"),
            ("running", "stopped"),
            ("completed", "running"),
            ("failed", "running"),
        ] {
            assert!(status_transition_allowed(from, to), "{from} -> {to}");
        }
    }

    #[test]
    fn re_writing_stopped_is_not_treated_as_a_regression() {
        assert!(status_transition_allowed("stopped", "stopped"));
    }
}
