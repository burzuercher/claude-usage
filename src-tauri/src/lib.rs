mod account;
mod attribution;
mod env;
mod pricing;
mod realusage;
mod usage;

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_positioner::{Position, WindowExt};

/// Shared state for the menu-bar popover behavior.
#[derive(Default)]
struct TrayState {
    /// The widget is pinned (always-on-top and exempt from click-outside hide).
    pinned: AtomicBool,
    /// Epoch-ms when a focus-loss last hid the window, so a tray-icon click that
    /// caused that same blur doesn't immediately reopen it.
    last_blur_hide_ms: AtomicI64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Aggregate usage from a specific environment's logs (defaults to `.claude`).
/// `session_start_ms` is the live session window start (reset − 5h) when the
/// frontend knows it, so the per-model split lines up with the ring.
#[tauri::command]
async fn get_usage(env_id: Option<String>, session_start_ms: Option<i64>) -> usage::UsageData {
    // Scanning the transcript logs is heavy blocking I/O; run it off the main
    // thread (and off the async reactor) so the UI never freezes while it runs.
    tauri::async_runtime::spawn_blocking(move || {
        let id = env_id.unwrap_or_default();
        usage::collect_at(env::projects_dir_for(&id), session_start_ms)
    })
    .await
    .unwrap_or_default()
}

/// Local attribution for the "what's contributing to your limits" view: which
/// behaviors, skills, subagents and MCP servers are driving usage, plus
/// per-session accounting. Machine-local and approximate, exactly as Claude
/// Code's own /usage screen is.
#[tauri::command]
async fn get_attribution(env_id: Option<String>) -> attribution::Attribution {
    // Same heavy transcript scan as get_usage — keep it off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let id = env_id.unwrap_or_default();
        attribution::collect_at(env::projects_dir_for(&id))
    })
    .await
    .unwrap_or_default()
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

/// Set whether the widget is pinned. The window floats above others whenever
/// it's shown (correct for a popover), so pinning only controls click-outside:
/// pinned keeps it open as a panel; unpinned lets a click elsewhere dismiss it.
/// Deliberately does not toggle the window level at runtime — doing so beachballs
/// a transparent, vibrant accessory window on macOS.
#[tauri::command]
fn set_pinned(state: tauri::State<TrayState>, pinned: bool) {
    state.pinned.store(pinned, Ordering::Relaxed);
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
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(TrayState::default())
        .setup(|app| {
            // macOS: run as an "accessory" — no Dock icon and no app menu, so the
            // widget lives only in the menu bar. The tray keeps it reachable.
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let window = app.get_webview_window("main").unwrap();
            // Default to the light backdrop; the frontend re-applies the saved theme.
            apply_window_effects(&window, false);
            create_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Close = hide to the tray/menu bar, keeping the app alive so the
            // tray icon can bring the widget back. Quit is from the tray menu.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Click-outside closes the popover — unless it's pinned, in which case
            // it stays open as a floating panel.
            tauri::WindowEvent::Focused(false) => {
                let state = window.state::<TrayState>();
                if !state.pinned.load(Ordering::Relaxed) {
                    state.last_blur_hide_ms.store(now_ms(), Ordering::Relaxed);
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_usage,
            get_attribution,
            get_account,
            get_real_usage,
            list_environments,
            set_pinned,
            set_window_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running claude-usage");
}

/// Show the widget as a popover anchored under the menu-bar icon, focused and
/// raised. Positioning happens while hidden to avoid a visible jump; it relies
/// on the tray icon's rect being cached by `positioner::on_tray_event`.
fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.move_window(Position::TrayCenter);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Build the menu-bar (macOS) / system-tray (Windows/Linux) icon: left-click
/// toggles the widget, and a menu offers Show and Quit. This is what keeps the
/// widget reachable after its window is closed to the tray.
fn create_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Claude Usage", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &hide, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("Claude Usage")
        .menu(&menu)
        // Let a left-click reach our TrayIconEvent handler instead of opening
        // the menu (which stays available on right-click).
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            // Cache the icon's screen rect so `Position::TrayCenter` can anchor
            // the popover under it.
            tauri_plugin_positioner::on_tray_event(app, &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = app.get_webview_window("main") {
                    let state = app.state::<TrayState>();
                    let visible = window.is_visible().unwrap_or(false);
                    // If a click-outside just hid the window via this same click's
                    // blur, treat the click as "close" rather than reopening it.
                    let just_blur_hid =
                        now_ms() - state.last_blur_hide_ms.load(Ordering::Relaxed) < 300;
                    if visible || just_blur_hid {
                        let _ = window.hide();
                    } else {
                        show_window(app);
                    }
                }
            }
        });

    // macOS: use a monochrome template glyph so the icon renders like the other
    // menu-bar icons (adapting to light/dark, and to the menu-bar tint). Other
    // platforms keep the full-color app icon.
    #[cfg(target_os = "macos")]
    {
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-macos.png"))?;
        builder = builder.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
    }

    builder.build(app)?;
    Ok(())
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
