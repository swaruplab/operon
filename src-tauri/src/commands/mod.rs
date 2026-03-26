pub mod terminal;
pub mod files;
pub mod claude;
pub mod ssh;
pub mod settings;
pub mod git;
pub mod knowledge;
pub mod extensions;

pub use terminal::{spawn_terminal, write_terminal, resize_terminal, kill_terminal};
pub use files::{list_directory, read_file, read_file_base64, write_file, save_clipboard_image, get_home_dir, create_file, create_directory, delete_path, rename_path, index_project, index_remote_project, list_protocols, read_protocol, get_protocols_dir, save_protocol, delete_protocol, generate_protocol};
pub use claude::{check_claude_installed, install_claude, store_api_key, get_api_key, delete_api_key, check_oauth_status, launch_claude_login, check_auth_status, start_claude_session, stop_claude_session, check_existing_plan, save_session_metadata, update_session_claude_id, update_session_status, list_sessions, check_session_files, read_session_output, reconnect_session, delete_session, rename_session, check_local_dependencies, install_xcode_cli, install_node, install_all_dependencies, install_phase_xcode, install_phase_tools, install_phase_claude, check_remote_claude, check_remote_claude_auth, install_remote_claude};
pub use ssh::{save_ssh_profile, list_ssh_profiles, get_server_config, detect_server_config, delete_ssh_profile, spawn_ssh_terminal, list_remote_directory, get_remote_home, read_remote_file, read_remote_file_base64, setup_ssh_key, test_ssh_connection, check_control_master, stop_control_master};
pub use settings::{get_settings, update_settings, start_dictation, stop_dictation};
pub use git::{git_status, git_init, git_commit_all, git_push, gh_check_auth, gh_install, gh_login, gh_create_repo, git_version_info, git_tag_version, git_publish};
pub use knowledge::search_pubmed;
pub use extensions::{search_extensions, get_extension_details, get_extension_manifest, get_extension_readme, get_namespace_extensions, get_extension_reviews, check_extension_compatibility, browse_extensions_by_category, list_installed_extensions, enable_extension, disable_extension, get_extension_package_json, install_extension_from_registry, uninstall_extension, sideload_vsix, read_extension_theme, read_extension_snippets, start_language_server, send_lsp_message, stop_language_server, list_language_servers};

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Operon.", name)
}

/// Open a URL in the user's default browser.
/// Writes the URL to a temp file and uses Python's webbrowser module to open it.
/// This avoids ALL encoding issues — the URL never passes through shell args,
/// HTML entity parsing, or AppleScript string handling.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    let tmp_dir = std::env::temp_dir();
    let url_file = tmp_dir.join("operon_oauth_url.txt");

    // Write the raw URL to a temp file — no encoding, no escaping
    std::fs::write(&url_file, &url)
        .map_err(|e| format!("Failed to write URL file: {}", e))?;

    // Use Python to read the URL from the file and open it in the browser.
    // This is the most reliable method because:
    // - The URL is read from a file, not passed as a CLI argument
    // - Python's webbrowser.open() handles all URL characters correctly
    // - No shell, HTML, or AppleScript string parsing involved
    let python_cmd = format!(
        "import webbrowser; url=open('{}').read().strip(); webbrowser.open(url)",
        url_file.to_string_lossy().replace('\'', "\\'")
    );

    std::process::Command::new("python3")
        .args(["-c", &python_cmd])
        .spawn()
        .map_err(|e| format!("Failed to open URL via Python: {}", e))?;

    Ok(())
}
