pub mod claude;
pub mod extensions;
pub mod files;
pub mod git;
pub mod knowledge;
pub mod mcp;
pub mod report;
pub mod settings;
pub mod ssh;
pub mod terminal;

pub use claude::{
    archive_current_plan, check_auth_status, check_claude_installed, check_existing_plan,
    check_local_dependencies, check_oauth_status, check_remote_claude, check_remote_claude_auth,
    check_session_files, delete_api_key, delete_session, get_api_key, install_all_dependencies,
    install_claude, install_node, install_phase_claude, install_phase_tools, install_phase_xcode,
    install_remote_claude, install_xcode_cli, launch_claude_login, list_plan_history,
    list_sessions, read_plan_history_entry, read_session_output, reconnect_session, reconnect_tail,
    refresh_environment, remote_claude_login, rename_session, save_session_metadata,
    start_claude_session, stop_claude_session, store_api_key, update_session_claude_id,
    update_session_status,
};
pub use extensions::{
    browse_extensions_by_category, check_extension_compatibility, check_extension_updates,
    disable_extension, docker_container_action, docker_list_containers, docker_list_images,
    docker_list_volumes, enable_extension, get_extension_config_schema, get_extension_details,
    get_extension_manifest, get_extension_package_json, get_extension_readme,
    get_extension_recommendations, get_extension_reviews, get_extension_settings,
    get_namespace_extensions, install_extension_from_registry, install_remote_extension,
    list_installed_extensions, list_language_servers, read_extension_snippets,
    read_extension_theme, search_extensions, send_lsp_message, sideload_vsix, singularity_action,
    singularity_list_images, singularity_list_instances, start_language_server,
    start_remote_language_server, stop_language_server, uninstall_extension,
    update_extension_settings, validate_extension_install,
};
pub use files::{
    check_remote_ripgrep, create_directory, create_file, delete_path, delete_protocol,
    generate_protocol, generate_protocol_from_files, get_home_dir, get_protocols_dir,
    index_project, index_remote_project, install_remote_ripgrep, list_directory,
    list_files_matching_regex, list_protocols, list_remote_files_matching_regex, read_file,
    read_file_base64, read_protocol, rename_path, save_attachment_file, save_clipboard_image,
    save_protocol, search_in_directory, search_in_remote_directory, write_file,
};
pub use git::{
    gh_add_remote, gh_check_auth, gh_create_repo, gh_install, gh_list_repos, gh_login, git_amend,
    git_changed_files, git_commit_all, git_discard_files, git_init, git_list_branches, git_log,
    git_publish, git_pull, git_push, git_show_commit, git_stage_files, git_stash_drop,
    git_stash_list, git_stash_pop, git_stash_save, git_status, git_switch_branch, git_tag_version,
    git_unstage_files, git_version_info,
};
pub use knowledge::search_pubmed;
pub use mcp::{
    add_mcp_server, check_mcp_dependencies, check_remote_mcp_dependencies, disable_mcp_server,
    enable_mcp_server, get_mcp_catalog, install_mcp_server, install_remote_mcp_server,
    list_mcp_servers, remove_mcp_server, update_mcp_server_env,
};
pub use report::{
    batch_read_file_previews, batch_read_remote_file_previews, extract_methods_info,
    generate_report_pdf, read_csv_for_report, scan_project_files, scan_remote_project_files,
};
pub use settings::{
    detect_custom_models, get_settings, start_dictation, stop_dictation, test_custom_endpoint,
    update_settings,
};
pub use ssh::{
    check_control_master, clear_ssh_cache, create_remote_directory, delete_remote_file,
    delete_ssh_profile, detect_server_config, get_remote_home, get_server_config,
    list_remote_directory, list_ssh_config_hosts, list_ssh_profiles, read_remote_file,
    read_remote_file_base64, rename_remote_path, save_ssh_profile, scp_batch_upload,
    scp_dir_from_remote, scp_from_remote, scp_to_remote, setup_ssh_key, spawn_ssh_terminal,
    stop_control_master, test_ssh_connection, write_remote_file,
};
pub use terminal::{
    get_terminal_cwd, kill_terminal, resize_terminal, spawn_terminal, write_terminal,
};

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Operon.", name)
}

/// Open a URL in the user's default browser.
/// Delegates to the platform abstraction layer for cross-platform support.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    crate::platform::open_url(&url)
}
