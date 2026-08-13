mod commands;
pub mod platform;

use commands::{
    add_mcp_server,
    archive_current_plan,
    batch_delete_files,
    batch_delete_remote_files,
    batch_read_file_previews,
    batch_read_remote_file_previews,
    browse_extensions_by_category,
    check_auth_status,
    // Claude Code
    check_claude_installed,
    check_control_master,
    check_existing_plan,
    check_extension_compatibility,
    // Phase 9: Polish & Reliability
    check_extension_updates,
    // Setup / Dependencies
    check_local_dependencies,
    check_mcp_dependencies,
    check_oauth_status,
    check_remote_claude,
    check_remote_claude_auth,
    check_remote_mcp_dependencies,
    check_remote_ripgrep,
    check_session_files,
    check_ssh_available,
    cleanup_legacy_watchdog,
    clear_session_state,
    clear_ssh_cache,
    create_directory,
    create_file,
    create_remote_directory,
    delete_api_key,
    delete_path,
    delete_protocol,
    delete_remote_file,
    delete_session,
    delete_ssh_profile,
    detect_custom_models,
    detect_server_config,
    disable_extension,
    disable_mcp_server,
    docker_container_action,
    // Docker & Singularity/Apptainer
    docker_list_containers,
    docker_list_images,
    docker_list_volumes,
    enable_extension,
    enable_mcp_server,
    extract_methods_info,
    fetch_anthropic_models,
    fetch_portkey_models,
    generate_protocol,
    generate_protocol_from_files,
    generate_report_pdf,
    get_api_key,
    get_cached_models,
    get_claude_invocation,
    get_extension_config_schema,
    get_extension_details,
    get_extension_manifest,
    get_extension_package_json,
    get_extension_readme,
    get_extension_recommendations,
    get_extension_reviews,
    get_extension_settings,
    get_home_dir,
    get_latest_claude_code_version,
    // MCP
    get_mcp_catalog,
    get_namespace_extensions,
    get_platform_info,
    get_protocol_template_params,
    get_protocols_dir,
    get_remote_home,
    get_remote_initial_dir,
    get_server_config,
    // Settings & System
    get_settings,
    get_ssh_diagnostics,
    // Terminal
    get_terminal_cwd,
    gh_add_remote,
    gh_check_auth,
    gh_create_repo,
    gh_install,
    gh_list_repos,
    gh_login,
    git_amend,
    git_changed_files,
    git_commit_all,
    git_discard_files,
    git_init,
    git_list_branches,
    git_log,
    git_publish,
    git_pull,
    git_push,
    git_show_commit,
    git_stage_files,
    git_stash_drop,
    git_stash_list,
    git_stash_pop,
    git_stash_save,
    // Git & GitHub
    git_status,
    git_switch_branch,
    git_tag_version,
    git_unstage_files,
    git_version_info,
    greet,
    index_project,
    index_remote_project,
    install_all_dependencies,
    install_claude,
    install_extension_from_registry,
    install_mcp_server,
    install_node,
    install_phase_claude,
    install_phase_tools,
    install_phase_xcode,
    install_remote_claude,
    install_remote_extension,
    install_remote_mcp_server,
    install_remote_ripgrep,
    install_xcode_cli,
    kill_terminal,
    launch_claude_login,
    // SLURM/PBS submission
    list_cluster_jobs,
    // Files
    list_directory,
    list_files_matching_regex,
    list_installed_extensions,
    list_language_servers,
    list_mcp_servers,
    list_pending_completions,
    list_plan_history,
    list_portkey_presets,
    // Protocols
    list_protocols,
    list_remote_directory,
    list_remote_files_matching_regex,
    list_sessions,
    list_ssh_config_hosts,
    list_ssh_profiles,
    load_session_state,
    mark_completion_seen,
    open_url,
    read_csv_for_report,
    read_extension_snippets,
    read_extension_theme,
    read_file,
    read_file_base64,
    read_job_log_tail,
    read_plan_history_entry,
    read_protocol,
    read_remote_file,
    read_remote_file_base64,
    read_review_events,
    read_session_output,
    reconnect_session,
    reconnect_tail,
    refresh_environment,
    refresh_models_if_stale,
    refresh_portkey_presets,
    register_slurm_job_metadata,
    remote_claude_login,
    remove_mcp_server,
    rename_path,
    rename_remote_path,
    rename_session,
    reorder_ssh_profiles,
    request_user_attention,
    reset_ssh_diagnostics,
    resize_terminal,
    resolve_remote_path,
    // Light code reviewer (Sonnet 5, checklist-driven)
    review_code,
    save_attachment_file,
    save_clipboard_image,
    save_protocol,
    // Session Management
    save_session_metadata,
    save_session_state,
    // SSH
    save_ssh_profile,
    // Report
    scan_project_files,
    scan_remote_footprint,
    scan_remote_project_files,
    scp_batch_upload,
    scp_dir_from_remote,
    scp_from_remote,
    scp_to_remote,
    // Extensions
    search_extensions,
    search_in_directory,
    search_in_remote_directory,
    // Knowledge Base
    search_pubmed,
    send_lsp_message,
    set_review_marker,
    setup_ssh_key,
    sftp_dir_download_with_progress,
    sftp_download_with_progress,
    sideload_vsix,
    singularity_action,
    singularity_list_images,
    singularity_list_instances,
    slurm_cancel_job,
    slurm_query_jobs,
    slurm_submit_job,
    // Terminal
    spawn_terminal,
    start_claude_session,
    start_dictation,
    start_language_server,
    start_remote_language_server,
    // Translation proxy (Anthropic ↔ OpenAI)
    start_translation_proxy,
    stop_claude_session,
    stop_control_master,
    stop_dictation,
    stop_language_server,
    stop_translation_proxy,
    store_api_key,
    teardown_remote_footprint,
    test_custom_endpoint,
    test_custom_endpoint_via_proxy,
    test_ssh_connection,
    translation_proxy_status,
    uninstall_extension,
    update_extension_settings,
    update_mcp_server_env,
    update_remote_claude,
    update_session_claude_id,
    update_session_status,
    update_settings,
    validate_extension_install,
    write_file,
    write_remote_file,
    write_terminal,
};
use tauri::{Emitter, Manager};

use commands::claude::ClaudeManager;
use commands::extensions::ExtensionManager;
use commands::job_notify::JobNotifyManager;
use commands::proxy::ProxyManager;
use commands::session::SessionStateManager;
use commands::settings::SettingsManager;
use commands::ssh::SSHManager;
use commands::ssh::{get_ssh_socket_path, prepare_ssh_auth};
use commands::sshauth::{
    add_ssh_key_passphrase, delete_ssh_key_passphrase, has_ssh_key_passphrase,
    key_needs_passphrase, set_ssh_key_passphrase,
};
use commands::terminal::TerminalManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(TerminalManager::new())
        .manage(ClaudeManager::new())
        .manage(SSHManager::new())
        .manage(SettingsManager::new())
        .manage(ExtensionManager::new())
        .manage(ProxyManager::new())
        .manage(JobNotifyManager::new())
        .manage(SessionStateManager::new())
        .setup(|app| {
            // Build platform-appropriate menu
            let menu = platform::build_menu(app)
                .map_err(|e| Box::new(std::io::Error::other(e.to_string())))?;
            app.set_menu(menu)?;

            // Handle menu events
            app.on_menu_event(move |app_handle, event| {
                if event.id().as_ref() == "open-help" {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.emit("open-help-panel", ());
                    }
                }
            });

            commands::ssh::start_wake_detector(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            // Terminal
            spawn_terminal,
            write_terminal,
            resize_terminal,
            kill_terminal,
            get_terminal_cwd,
            // Files
            list_directory,
            read_file,
            read_file_base64,
            write_file,
            save_clipboard_image,
            save_attachment_file,
            get_home_dir,
            create_file,
            create_directory,
            delete_path,
            batch_delete_files,
            rename_path,
            index_project,
            index_remote_project,
            search_in_directory,
            search_in_remote_directory,
            check_remote_ripgrep,
            install_remote_ripgrep,
            list_files_matching_regex,
            list_remote_files_matching_regex,
            // Protocols
            list_protocols,
            read_protocol,
            get_protocols_dir,
            save_protocol,
            delete_protocol,
            generate_protocol,
            generate_protocol_from_files,
            get_protocol_template_params,
            // Claude Code
            check_claude_installed,
            get_claude_invocation,
            check_ssh_available,
            install_claude,
            store_api_key,
            get_api_key,
            delete_api_key,
            check_oauth_status,
            launch_claude_login,
            check_auth_status,
            start_claude_session,
            stop_claude_session,
            scan_remote_footprint,
            teardown_remote_footprint,
            check_existing_plan,
            archive_current_plan,
            list_plan_history,
            read_plan_history_entry,
            // Session Management
            save_session_metadata,
            update_session_claude_id,
            update_session_status,
            list_sessions,
            check_session_files,
            read_session_output,
            reconnect_session,
            reconnect_tail,
            delete_session,
            rename_session,
            // Setup / Dependencies
            check_local_dependencies,
            refresh_environment,
            install_xcode_cli,
            install_node,
            install_all_dependencies,
            install_phase_xcode,
            install_phase_tools,
            install_phase_claude,
            check_remote_claude,
            check_remote_claude_auth,
            get_latest_claude_code_version,
            update_remote_claude,
            install_remote_claude,
            remote_claude_login,
            // SSH
            save_ssh_profile,
            list_ssh_profiles,
            list_ssh_config_hosts,
            get_server_config,
            detect_server_config,
            delete_ssh_profile,
            reorder_ssh_profiles,
            resolve_remote_path,
            list_remote_directory,
            get_remote_home,
            get_remote_initial_dir,
            read_remote_file,
            read_remote_file_base64,
            create_remote_directory,
            delete_remote_file,
            batch_delete_remote_files,
            rename_remote_path,
            write_remote_file,
            scp_to_remote,
            scp_from_remote,
            scp_dir_from_remote,
            scp_batch_upload,
            sftp_download_with_progress,
            sftp_dir_download_with_progress,
            clear_ssh_cache,
            setup_ssh_key,
            test_ssh_connection,
            get_ssh_socket_path,
            prepare_ssh_auth,
            set_ssh_key_passphrase,
            add_ssh_key_passphrase,
            delete_ssh_key_passphrase,
            has_ssh_key_passphrase,
            key_needs_passphrase,
            check_control_master,
            stop_control_master,
            get_ssh_diagnostics,
            reset_ssh_diagnostics,
            // Settings
            get_settings,
            update_settings,
            detect_custom_models,
            test_custom_endpoint,
            test_custom_endpoint_via_proxy,
            // Models catalog (auto-fetched from Anthropic /v1/models)
            fetch_anthropic_models,
            get_cached_models,
            refresh_models_if_stale,
            // Portkey gateway (Operon 0.7.0)
            list_portkey_presets,
            refresh_portkey_presets,
            fetch_portkey_models,
            // Translation proxy
            start_translation_proxy,
            stop_translation_proxy,
            translation_proxy_status,
            // Git & GitHub
            git_status,
            git_init,
            git_commit_all,
            git_push,
            gh_check_auth,
            gh_install,
            gh_login,
            gh_create_repo,
            git_version_info,
            git_tag_version,
            git_publish,
            gh_list_repos,
            gh_add_remote,
            git_list_branches,
            git_switch_branch,
            git_pull,
            git_changed_files,
            git_stage_files,
            git_unstage_files,
            git_discard_files,
            git_stash_list,
            git_stash_save,
            git_stash_pop,
            git_stash_drop,
            git_log,
            git_show_commit,
            git_amend,
            // Knowledge Base
            search_pubmed,
            start_dictation,
            stop_dictation,
            // Extensions
            search_extensions,
            get_extension_details,
            get_extension_manifest,
            get_extension_readme,
            get_namespace_extensions,
            get_extension_reviews,
            check_extension_compatibility,
            browse_extensions_by_category,
            list_installed_extensions,
            enable_extension,
            disable_extension,
            get_extension_package_json,
            install_extension_from_registry,
            uninstall_extension,
            sideload_vsix,
            read_extension_theme,
            read_extension_snippets,
            // LSP
            start_language_server,
            send_lsp_message,
            stop_language_server,
            list_language_servers,
            // Remote LSP
            start_remote_language_server,
            // Remote Extensions
            install_remote_extension,
            // Extension Settings
            get_extension_config_schema,
            get_extension_settings,
            update_extension_settings,
            // Phase 9: Polish & Reliability
            check_extension_updates,
            get_extension_recommendations,
            validate_extension_install,
            // Docker & Singularity/Apptainer
            docker_list_containers,
            docker_list_images,
            docker_list_volumes,
            docker_container_action,
            singularity_list_images,
            singularity_list_instances,
            singularity_action,
            // MCP
            get_mcp_catalog,
            list_mcp_servers,
            add_mcp_server,
            remove_mcp_server,
            enable_mcp_server,
            disable_mcp_server,
            update_mcp_server_env,
            install_mcp_server,
            check_mcp_dependencies,
            check_remote_mcp_dependencies,
            install_remote_mcp_server,
            // Report
            scan_project_files,
            scan_remote_project_files,
            extract_methods_info,
            read_csv_for_report,
            generate_report_pdf,
            batch_read_file_previews,
            batch_read_remote_file_previews,
            // Utilities
            open_url,
            get_platform_info,
            // Light code reviewer
            review_code,
            read_review_events,
            set_review_marker,
            // HPC job tracking. The login-node watchdog daemon was removed; job
            // state comes from list_cluster_jobs (squeue + sacct) on demand.
            cleanup_legacy_watchdog,
            // Job completion notifications (0.6.8)
            register_slurm_job_metadata,
            list_pending_completions,
            mark_completion_seen,
            request_user_attention,
            // Last-session restore (Operon 0.7.x — task #72)
            save_session_state,
            load_session_state,
            clear_session_state,
            // SLURM / PBS job submission (task #71)
            slurm_submit_job,
            slurm_query_jobs,
            slurm_cancel_job,
            list_cluster_jobs,
            read_job_log_tail,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Kill all terminal processes
                let state = window.state::<TerminalManager>();
                let terminals = state.terminals.lock();
                if let Ok(terminals) = terminals {
                    for handle in terminals.values() {
                        if let Ok(mut child) = handle.child.lock() {
                            let _ = child.kill();
                        }
                    }
                }
                // Kill all local Claude/node agent sessions (otherwise they
                // orphan and keep running after the window closes).
                window.state::<ClaudeManager>().kill_all();
                // Kill the translation proxy sidecar if running
                let proxy = window.state::<ProxyManager>();
                let _ = proxy.stop();
                // Kill the ssh-agent Operon spawned for passphrase-protected keys
                // (no-op if we reused an existing agent).
                commands::sshauth::shutdown_agent();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Every Tauri command registered for the frontend must actually be reachable
/// from it.
///
/// This is the check that would have caught SLU-01: `register_slurm_job_metadata`
/// was implemented, registered here, wrapped in TypeScript — and called by nothing.
/// Completion notifications therefore never fired, while the UI and the agent
/// prompt both told users they would. Nothing failed; the feature was simply
/// disconnected, and it stayed that way through several releases.
///
/// A command with no caller is not always a bug — some are invoked only from
/// other Rust code, or exist for an extension surface. Those go in
/// `CALLED_FROM_RUST_ONLY` with a reason, so the exemption is a deliberate,
/// reviewable act rather than silence.
#[cfg(test)]
mod command_wiring_tests {
    /// Commands intentionally not invoked from `src/`. Keep the reason attached.
    const CALLED_FROM_RUST_ONLY: &[(&str, &str)] = &[];

    /// Commands that were registered but never wired to the UI, found when this
    /// check was first written. They are recorded rather than silently tolerated:
    /// each is either a feature that was built and never connected (the SLU-01
    /// shape — the completion-notification command sat here for releases while the
    /// UI claimed it worked) or dead weight that should be deleted.
    ///
    /// This list may shrink. It must not grow: a NEW entry means someone shipped
    /// another disconnected feature, which is exactly what the test exists to stop.
    const KNOWN_UNWIRED: &[&str] = &[
        // Tauri template leftover.
        "greet",
        // Session management implemented backend-only; the chat panel manages
        // sessions through other commands.
        "save_session_metadata",
        "list_sessions",
        "check_session_files",
        "reconnect_session",
        "delete_session",
        "rename_session",
        "archive_current_plan",
        // Setup/install paths the wizard reaches by other routes.
        "install_xcode_cli",
        "install_node",
        "remote_claude_login",
        "check_remote_ripgrep",
        "get_protocol_template_params",
        // Git write operations — exposed, never called from the UI.
        "git_commit_all",
        "git_push",
        "git_tag_version",
    ];

    /// Pull the identifiers out of the `tauri::generate_handler![...]` list.
    fn registered_commands(src: &str) -> Vec<String> {
        let start = match src.find("generate_handler!") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let open = match src[start..].find('[') {
            Some(i) => start + i + 1,
            None => return Vec::new(),
        };
        let close = match src[open..].find(']') {
            Some(i) => open + i,
            None => return Vec::new(),
        };
        src[open..close]
            .split(',')
            .map(|s| {
                // Strip comments and whitespace; keep the bare identifier.
                s.lines()
                    .map(|l| l.split("//").next().unwrap_or("").trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .map(|s| s.rsplit("::").next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .collect()
    }

    fn frontend_sources() -> String {
        fn walk(dir: &std::path::Path, out: &mut String) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if matches!(
                    p.extension().and_then(|s| s.to_str()),
                    Some("ts") | Some("tsx")
                ) {
                    if let Ok(s) = std::fs::read_to_string(&p) {
                        out.push_str(&s);
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("src"));
        let mut out = String::new();
        if let Some(root) = root {
            walk(&root, &mut out);
        }
        out
    }

    #[test]
    fn every_registered_command_has_a_caller() {
        let commands = registered_commands(include_str!("lib.rs"));
        assert!(
            commands.len() > 50,
            "failed to parse the handler list — got {} commands, so this test \
             would pass vacuously",
            commands.len()
        );

        let frontend = frontend_sources();
        assert!(
            frontend.len() > 100_000,
            "failed to read the frontend sources — got {} bytes",
            frontend.len()
        );

        let mut exempt: Vec<&str> = CALLED_FROM_RUST_ONLY.iter().map(|(n, _)| *n).collect();
        exempt.extend_from_slice(KNOWN_UNWIRED);
        let orphans: Vec<&String> = commands
            .iter()
            .filter(|c| !exempt.contains(&c.as_str()))
            // The name appears in an invoke() call, a wrapper, or a listen() target.
            .filter(|c| !frontend.contains(c.as_str()))
            .collect();

        // If a KNOWN_UNWIRED command gains a caller, delete it from the list — the
        // debt is paid and leaving it there would mask a future regression.
        let now_wired: Vec<&&str> = KNOWN_UNWIRED
            .iter()
            .filter(|c| frontend.contains(**c))
            .collect();
        assert!(
            now_wired.is_empty(),
            "these commands are now called from src/ — remove them from KNOWN_UNWIRED: {now_wired:?}"
        );

        assert!(
            orphans.is_empty(),
            "these Tauri commands are registered but never referenced from src/ — \
             either wire them up or add them to CALLED_FROM_RUST_ONLY with a reason: \
             {orphans:?}"
        );
    }
}
