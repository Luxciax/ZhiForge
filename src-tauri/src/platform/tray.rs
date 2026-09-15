use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Listener,
};

use crate::{platform::{autostart, window}, selection};

pub fn setup(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开 ZhiForge", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(
        app,
        "pause-selection",
        "暂停划词",
        true,
        selection::selection_paused(app.handle()),
        None::<&str>,
    )?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机启动",
        true,
        autostart::is_enabled(app.handle()).unwrap_or(false),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &pause, &autostart_item, &quit])?;

    let pause_listener = pause.clone();
    app.listen("runtime://pause-changed", move |event| {
        if let Ok(paused) = serde_json::from_str::<bool>(event.payload()) {
            let _ = pause_listener.set_checked(paused);
        }
    });
    let autostart_listener = autostart_item.clone();
    app.listen("runtime://autostart-changed", move |event| {
        if let Ok(enabled) = serde_json::from_str::<bool>(event.payload()) {
            let _ = autostart_listener.set_checked(enabled);
        }
    });

    let pause_menu = pause.clone();
    let autostart_menu = autostart_item.clone();
    TrayIconBuilder::new()
        .icon(app.default_window_icon().expect("application icon is missing").clone())
        .tooltip("ZhiForge")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => window::show_home(app),
            "settings" => window::show_settings(app),
            "pause-selection" => {
                let next = !selection::selection_paused(app);
                if selection::set_selection_paused(app, next).is_ok() {
                    let _ = pause_menu.set_checked(next);
                }
            }
            "autostart" => {
                let next = !autostart::is_enabled(app).unwrap_or(false);
                if let Ok(enabled) = autostart::set_enabled(app, next) {
                    let _ = autostart_menu.set_checked(enabled);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_home(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
