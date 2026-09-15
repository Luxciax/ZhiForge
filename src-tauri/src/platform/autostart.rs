use tauri::{AppHandle, Emitter};
use tauri_plugin_autostart::ManagerExt;

pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| format!("failed to read autostart state: {error}"))
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|error| format!("failed to enable autostart: {error}"))?;
    } else {
        manager
            .disable()
            .map_err(|error| format!("failed to disable autostart: {error}"))?;
    }
    let actual = is_enabled(app)?;
    let _ = app.emit("runtime://autostart-changed", actual);
    Ok(actual)
}

#[tauri::command]
pub fn autostart_is_enabled(app: AppHandle) -> Result<bool, String> {
    is_enabled(&app)
}

#[tauri::command]
pub fn autostart_set_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    set_enabled(&app, enabled)
}
