use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewWindow};

const MAIN_WINDOW_WIDTH: f64 = 1120.0;
const MAIN_WINDOW_HEIGHT: f64 = 760.0;

#[cfg(target_os = "windows")]
fn native_hwnd(window: &WebviewWindow) -> Result<windows::Win32::Foundation::HWND, String> {
    use windows::Win32::Foundation::HWND;

    let tauri_hwnd = window.hwnd().map_err(|error| error.to_string())?;
    Ok(HWND(tauri_hwnd.0))
}

#[cfg(target_os = "windows")]
fn restore_native_window(window: &WebviewWindow) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, ShowWindow, SW_RESTORE};

    let Ok(hwnd) = native_hwnd(window) else {
        // During Tauri setup the WebViewWindow can exist before its native HWND is ready.
        // Native restore is only an extra minimized-window recovery path, so it must not
        // prevent the normal Tauri show/focus sequence on a fresh launch.
        return Ok(());
    };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn restore_native_window(_window: &WebviewWindow) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn focus_native_window(window: &WebviewWindow) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let hwnd = native_hwnd(window)?;
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn focus_native_window(_window: &WebviewWindow) -> Result<(), String> {
    Ok(())
}

pub fn restore_window_state(window: &WebviewWindow) -> Result<(), String> {
    restore_native_window(window)?;
    if window.is_minimized().unwrap_or(false) {
        window.unminimize().map_err(|error| error.to_string())?;
    }
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn show_and_focus(window: &WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    let _ = focus_native_window(window);
    window.set_focus().map_err(|error| error.to_string())
}

fn prepare_main_window(window: &WebviewWindow) -> Result<(), String> {
    restore_window_state(window)?;
    window
        .set_size(LogicalSize::new(MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT))
        .map_err(|error| error.to_string())
}

pub fn launched_from_autostart() -> bool {
    std::env::args().any(|arg| arg == "--autostart")
}

pub fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("result")
        .ok_or_else(|| "result window is unavailable".to_string())
}

pub fn show_window(window: &WebviewWindow) -> Result<(), String> {
    prepare_main_window(window)?;
    window.center().map_err(|error| error.to_string())?;
    show_and_focus(window)
}

pub fn show_home(app: &AppHandle) {
    match main_window(app) {
        Ok(window) => {
            if let Err(error) = window.emit("app://home", ()) {
                eprintln!("failed to emit home event: {error}");
            }
            if let Err(error) = show_window(&window) {
                eprintln!("failed to show ZhiForge main window: {error}");
            }
        }
        Err(error) => eprintln!("failed to resolve ZhiForge main window: {error}"),
    }
}

pub fn show_settings(app: &AppHandle) {
    match main_window(app) {
        Ok(window) => {
            if let Err(error) = window.emit("app://home", ()) {
                eprintln!("failed to emit home event before settings: {error}");
            }
            if let Err(error) = window.emit("app://settings", ()) {
                eprintln!("failed to emit settings event: {error}");
            }
            if let Err(error) = show_window(&window) {
                eprintln!("failed to show ZhiForge settings window: {error}");
            }
        }
        Err(error) => eprintln!("failed to resolve ZhiForge settings window: {error}"),
    }
}
