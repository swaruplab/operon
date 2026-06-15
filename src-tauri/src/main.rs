// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Work around the WebKitGTK blank-window bug on Wayland/NVIDIA/VMs, where the
    // DMABUF renderer produces an all-white (or black) webview. Must be set before
    // the webview initializes. Respect an existing user override if one is present.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    // Enable per-monitor DPI awareness on Windows.
    // Without this, the WebView2 renders at 96 DPI and then upscales,
    // causing blurry/hazy text on high-DPI displays.
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn SetProcessDPIAware() -> i32;
        }
        SetProcessDPIAware();
    }

    operon_lib::run()
}
