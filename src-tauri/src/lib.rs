mod commands;
pub mod platform;

use tauri::{Emitter, Manager};
use commands::{
    greet,
    // Terminal
    spawn_terminal, write_terminal, resize_terminal, kill_terminal,
    // Files
    list_directory, read_file, read_file_base64, write_file, save_clipboard_image, save_attachment_file, get_home_dir,
    create_file, create_directory, delete_path, rename_path,
    index_project, index_remote_project,
    // Protocols
    list_protocols, read_protocol, get_protocols_dir, save_protocol, delete_protocol, generate_protocol,
    // Claude Code
    check_claude_installed, install_claude,
    store_api_key, get_api_key, delete_api_key,
    check_oauth_status, launch_claude_login, check_auth_status,
    start_claude_session, stop_claude_session, check_existing_plan, archive_current_plan,
    list_plan_history, read_plan_history_entry,
    // Session Management
    save_session_metadata, update_session_claude_id, update_session_status,
    list_sessions, check_session_files, read_session_output,
    reconnect_session, delete_session, rename_session,
    // Setup / Dependencies
    check_local_dependencies, refresh_environment, install_xcode_cli, install_node, install_all_dependencies,
    install_phase_xcode, install_phase_tools, install_phase_claude,
    check_remote_claude, check_remote_claude_auth, install_remote_claude,
    // SSH
    save_ssh_profile, list_ssh_profiles, get_server_config, detect_server_config,
    delete_ssh_profile, spawn_ssh_terminal,
    list_remote_directory, get_remote_home, read_remote_file, read_remote_file_base64,
    create_remote_directory, write_remote_file, scp_to_remote,
    setup_ssh_key, test_ssh_connection, check_control_master, stop_control_master,
    // Settings & System
    get_settings, update_settings, start_dictation, stop_dictation,
    open_url,
    // Git & GitHub
    git_status, git_init, git_commit_all, git_push,
    gh_check_auth, gh_install, gh_login, gh_create_repo,
    git_version_info, git_tag_version, git_publish,
    // Knowledge Base
    search_pubmed,
    // Extensions
    search_extensions, get_extension_details, get_extension_manifest,
    get_extension_readme, get_namespace_extensions, get_extension_reviews,
    check_extension_compatibility, browse_extensions_by_category,
    list_installed_extensions, enable_extension, disable_extension,
    get_extension_package_json, install_extension_from_registry,
    uninstall_extension, sideload_vsix, read_extension_theme, read_extension_snippets,
    start_language_server, send_lsp_message, stop_language_server, list_language_servers,
    get_extension_config_schema, get_extension_settings, update_extension_settings,
    start_remote_language_server, install_remote_extension,
    // Phase 9: Polish & Reliability
    check_extension_updates, get_extension_recommendations, validate_extension_install,
    // Docker & Singularity/Apptainer
    docker_list_containers, docker_list_images, docker_list_volumes, docker_container_action,
    singularity_list_images, singularity_list_instances, singularity_action,
    // MCP
    get_mcp_catalog, list_mcp_servers, add_mcp_server, remove_mcp_server,
    enable_mcp_server, disable_mcp_server, update_mcp_server_env, install_mcp_server,
    check_mcp_dependencies, check_remote_mcp_dependencies, install_remote_mcp_server,
    // Report
    scan_project_files, scan_remote_project_files, extract_methods_info,
    read_csv_for_report, generate_report_pdf,
    batch_read_file_previews, batch_read_remote_file_previews,
};

use commands::terminal::TerminalManager;
use commands::claude::ClaudeManager;
use commands::ssh::SSHManager;
use commands::settings::SettingsManager;
use commands::extensions::ExtensionManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(TerminalManager::new())
        .manage(ClaudeManager::new())
        .manage(SSHManager::new())
        .manage(SettingsManager::new())
        .manage(ExtensionManager::new())
        .setup(|app| {
            // Build platform-appropriate menu
            let menu = platform::build_menu(app)
                .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            app.set_menu(menu)?;

            // Handle menu events
            app.on_menu_event(move |app_handle, event| {
                if event.id().as_ref() == "open-help" {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.emit("open-help-panel", ());
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            // Terminal
            spawn_terminal,
            write_terminal,
            resize_terminal,
            kill_terminal,
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
            rename_path,
            index_project,
            index_remote_project,
            // Protocols
            list_protocols,
            read_protocol,
            get_protocols_dir,
            save_protocol,
            delete_protocol,
            generate_protocol,
            // Claude Code
            check_claude_installed,
            install_claude,
            store_api_key,
            get_api_key,
            delete_api_key,
            check_oauth_status,
            launch_claude_login,
            check_auth_status,
            start_claude_session,
            stop_claude_session,
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
            install_remote_claude,
            // SSH
            save_ssh_profile,
            list_ssh_profiles,
            get_server_config,
            detect_server_config,
            delete_ssh_profile,
            spawn_ssh_terminal,
            list_remote_directory,
            get_remote_home,
            read_remote_file,
            read_remote_file_base64,
            create_remote_directory,
            write_remote_file,
            scp_to_remote,
            setup_ssh_key,
            test_ssh_connection,
            check_control_master,
            stop_control_master,
            // Settings
            get_settings,
            update_settings,
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
            // Knowledge Base
            search_pubmed,
            start_dictation, stop_dictation,
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
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Kill all terminal processes
                let state = window.state::<TerminalManager>();
                let terminals = state.terminals.lock();
                if let Ok(terminals) = terminals {
                    for (_, handle) in terminals.iter() {
                        if let Ok(mut child) = handle.child.lock() {
                            let _ = child.kill();
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
