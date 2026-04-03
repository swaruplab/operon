// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
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
