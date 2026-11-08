#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Earshot - footstep equalizer that switches on when a shooter starts.

mod analyzer;
mod apo;
mod commands;
mod dsp;
mod profiles;
mod state;
mod watcher;

use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent, Wry};

const TRAY_ID: &str = "earshot-tray";

/// Menu item that shows the current profile in the tray menu.
struct TrayStatus(MenuItem<Wry>);

pub fn set_tray_status(app: &AppHandle, text: &str) {
    if let Some(item) = app.try_state::<TrayStatus>() {
        let _ = item.0.set_text(text);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("Earshot - {text}")));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let h = app.handle();
    let status = MenuItem::with_id(h, "status", "Flat - Flat (desktop)", false, None::<&str>)?;
    let bypass = MenuItem::with_id(h, "bypass", "A/B bypass", true, Some("Ctrl+Alt+B"))?;
    let open = MenuItem::with_id(h, "open", "Open Earshot", true, None::<&str>)?;
    let quit = MenuItem::with_id(h, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(h)?;
    let menu = Menu::with_items(h, &[&status, &sep, &bypass, &open, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Earshot")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "bypass" => {
                state::toggle_bypass(app);
            }
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayStatus(status));
    Ok(())
}

fn main() {
    let launched_minimized = std::env::args().any(|a| a == "--minimized");
    tauri::Builder::default()
        .manage(state::AppState::load())
        .manage(Mutex::new(analyzer::Synth::new(0x5EED_F00D)))
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_curve,
            commands::set_band,
            commands::commit_edit,
            commands::set_preset,
            commands::set_auto,
            commands::activate_profile,
            commands::toggle_bypass,
            commands::apply_now,
            commands::preview_config,
            commands::remove_from_apo,
            commands::refresh_apo,
            commands::list_devices,
            commands::update_settings,
            commands::spectrum_frame,
            commands::import_preset,
        ])
        .setup(move |app| {
            build_tray(app)?;
            let handle = app.handle().clone();
            let hotkey = app.state::<state::AppState>().lock().settings.bypass_hotkey.clone();
            watcher::spawn_process_watcher(handle.clone());
            watcher::spawn_hotkey(handle.clone(), hotkey);
            state::broadcast(&handle);
            if launched_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window.app_handle().state::<state::AppState>().lock().settings.minimize_to_tray;
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Earshot");
}
