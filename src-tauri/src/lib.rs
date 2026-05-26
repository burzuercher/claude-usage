mod account;
mod env;
mod pricing;
mod realusage;
mod usage;

use tauri::Manager;

/// Aggregate usage from a specific environment's logs (defaults to `.claude`).
#[tauri::command]
fn get_usage(env_id: Option<String>) -> usage::UsageData {
    let id = env_id.unwrap_or_default();
    usage::collect_at(env::projects_dir_for(&id))
}

/// List discovered Claude environments (separate `~/.claude*` config dirs).
#[tauri::command]
fn list_environments() -> Vec<env::Environment> {
    env::list()
}

/// Read the account/plan for a given environment (discovered name or path).
#[tauri::command]
fn get_account(env_id: Option<String>) -> account::Account {
    env::account_for(&env_id.unwrap_or_default())
}

/// Fetch authoritative usage limits from Anthropic for a given environment
/// (the same data the Claude app shows). Token is used at runtime only.
#[tauri::command]
async fn get_real_usage(env_id: Option<String>) -> realusage::RealUsage {
    realusage::fetch(&env_id.unwrap_or_default()).await
}

/// Toggle the widget's always-on-top behavior (driven by the pin button).
#[tauri::command]
fn set_pinned(window: tauri::Window, pinned: bool) -> Result<(), String> {
    window.set_always_on_top(pinned).map_err(|e| e.to_string())
}

/// Re-apply the window backdrop to match the app's light/dark theme. Without
/// this, a light app on a dark-themed OS gets a muddy dark Mica tint.
#[tauri::command]
fn set_window_theme(window: tauri::WebviewWindow, dark: bool) {
    apply_window_effects(&window, dark);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            // Default to the light backdrop; the frontend re-applies the saved theme.
            apply_window_effects(&window, false);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_usage,
            get_account,
            get_real_usage,
            list_environments,
            set_pinned,
            set_window_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running claude-usage");
}

/// Apply a translucent Mica/vibrancy backdrop matching the app theme, so the
/// warm palette reads as a frosted Windows 11 widget. Passing the theme avoids a
/// dark Mica tint bleeding over the light UI on dark-themed systems.
/// No-op on Linux (solid `--paper` shows through).
fn apply_window_effects(window: &tauri::WebviewWindow, dark: bool) {
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::apply_mica;
        // Some(dark) forces the Mica variant to match the UI.
        let _ = apply_mica(window, Some(dark));
    }

    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
        let material = if dark {
            NSVisualEffectMaterial::HudWindow
        } else {
            NSVisualEffectMaterial::ContentBackground
        };
        let _ = apply_vibrancy(window, material, None, Some(12.0));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (window, dark);
    }
}
