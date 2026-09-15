mod clipboard;
mod filters;
mod gesture;
mod hook;
mod msaa;
mod position;
mod resolver;
mod types;
mod uia;

use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{channel, sync_channel},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Position};

use crate::settings::SettingsState;
use gesture::GestureEngine;
use hook::HookHandle;
use types::{InputEvent, SelectionCandidate};

struct HookState {
    _hook: Mutex<Option<HookHandle>>,
}

pub struct SelectionRuntime {
    paused: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl SelectionRuntime {
    fn new() -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug)]
struct QueuedCandidate {
    generation: u64,
    candidate: SelectionCandidate,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionSelection {
    text: String,
    program_name: String,
    method: String,
    mouse_x: i32,
    mouse_y: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_mode: Option<String>,
    selection: ActionSelection,
}

#[tauri::command]
pub fn open_action(app: AppHandle, request: ActionRequest) -> Result<(), String> {
    let window = app
        .get_webview_window("result")
        .ok_or_else(|| "result window is unavailable".to_string())?;

    crate::platform::window::restore_window_state(&window)?;
    window
        .set_size(LogicalSize::new(560.0, 420.0))
        .map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let anchor = types::Point {
        x: request.selection.mouse_x,
        y: request.selection.mouse_y,
    };
    let point = position::result_position(anchor, size.width as i32, size.height as i32);

    window
        .set_position(Position::Physical(PhysicalPosition::new(point.x, point.y)))
        .map_err(|error| error.to_string())?;
    window.emit("selection://action", request).map_err(|error| error.to_string())?;
    crate::platform::window::show_and_focus(&window)?;
    Ok(())
}

#[tauri::command]
pub fn hide_result(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("result")
        .ok_or_else(|| "result window is unavailable".to_string())?;
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_result_pinned(app: AppHandle, pinned: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("result")
        .ok_or_else(|| "result window is unavailable".to_string())?;
    window.set_always_on_top(pinned).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn write_to_clipboard(text: String) -> Result<(), String> {
    clipboard::write_text(&text)
}

pub fn set_selection_paused(app: &AppHandle, paused: bool) -> Result<bool, String> {
    let state = app.state::<SelectionRuntime>();
    state.set_paused(paused);
    if paused {
        hide_toolbar(app);
    }
    let _ = app.emit("runtime://pause-changed", paused);
    Ok(paused)
}

pub fn selection_paused(app: &AppHandle) -> bool {
    app.state::<SelectionRuntime>().is_paused()
}

#[tauri::command]
pub fn runtime_pause_selection(app: AppHandle) -> Result<bool, String> {
    set_selection_paused(&app, true)
}

#[tauri::command]
pub fn runtime_resume_selection(app: AppHandle) -> Result<bool, String> {
    set_selection_paused(&app, false)
}

#[tauri::command]
pub fn runtime_selection_paused(app: AppHandle) -> bool {
    selection_paused(&app)
}

pub fn start(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let (input_tx, input_rx) = sync_channel::<InputEvent>(128);
    let (candidate_tx, candidate_rx) = channel::<QueuedCandidate>();
    let runtime = SelectionRuntime::new();
    let latest_generation = runtime.generation.clone();
    let event_paused = runtime.paused.clone();
    let resolver_paused = runtime.paused.clone();
    app.manage(runtime);
    let hook = hook::start(input_tx).map_err(std::io::Error::other)?;
    app.manage(HookState {
        _hook: Mutex::new(Some(hook)),
    });

    let event_app = app.clone();
    let event_generation = latest_generation.clone();
    std::thread::Builder::new()
        .name("selection-gesture".into())
        .spawn(move || {
            let double_click_ms = windows_double_click_time();
            let mut gestures = GestureEngine::new(double_click_ms);
            while let Ok(event) = input_rx.recv() {
                if event_paused.load(Ordering::Acquire) {
                    gestures.reset();
                    continue;
                }
                let trigger = event_app
                    .state::<SettingsState>()
                    .trigger_profile()
                    .unwrap_or_default();

                // Invalidate any selection still resolving as soon as the user starts
                // a new pointer interaction, not only after the next selection finishes.
                if let InputEvent::LeftDown(snapshot) = &event {
                    event_generation.fetch_add(1, Ordering::AcqRel);
                    hide_toolbar_if_outside(&event_app, snapshot.point);
                }

                match &event {
                    InputEvent::MouseWheel => hide_toolbar(&event_app),
                    InputEvent::KeyDown { vk_code } if should_hide_for_key(*vk_code) => hide_toolbar(&event_app),
                    _ => {}
                }
                if let Some(candidate) = gestures.handle(&event, &trigger) {
                    let generation = event_generation.load(Ordering::Acquire);
                    let _ = candidate_tx.send(QueuedCandidate { generation, candidate });
                }
            }
        })?;

    let resolver_app = app.clone();
    let resolver_generation = latest_generation.clone();
    std::thread::Builder::new()
        .name("selection-resolver".into())
        .spawn(move || {
            while let Ok(mut queued) = candidate_rx.recv() {
                // If several selections arrived while the previous one was resolving,
                // skip directly to the newest candidate instead of replaying stale work.
                for newer in candidate_rx.try_iter() {
                    queued = newer;
                }

                if resolver_paused.load(Ordering::Acquire) {
                    continue;
                }
                let app_rules = resolver_app
                    .state::<SettingsState>()
                    .app_rules()
                    .unwrap_or_default();
                let Some(payload) = resolver::resolve(queued.candidate, &app_rules) else {
                    continue;
                };

                // A newer gesture may have arrived during UIA/MSAA/clipboard resolution.
                // Never flash or log the stale result back onto the screen.
                if queued.generation != resolver_generation.load(Ordering::Acquire)
                    || resolver_paused.load(Ordering::Acquire)
                {
                    continue;
                }
                resolver_app.state::<crate::diagnostics::DiagnosticLog>().record(
                    "selection.resolved",
                    &format!("app={} method={}", payload.program_name, payload.method),
                );

                if let Some(window) = resolver_app.get_webview_window("toolbar") {
                    let size = window.outer_size().unwrap_or_else(|_| tauri::PhysicalSize::new(350, 43));
                    let position = position::toolbar_position(
                        queued.candidate,
                        size.width as i32,
                        size.height as i32,
                    );
                    let _ = window.set_position(Position::Physical(PhysicalPosition::new(position.x, position.y)));
                    let _ = window.emit("selection://changed", payload);
                    let _ = window.show();
                }
            }
        })?;

    Ok(())
}

fn hide_toolbar(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("toolbar") {
        let _ = window.hide();
    }
}

fn hide_toolbar_if_outside(app: &AppHandle, point: types::Point) {
    let Some(window) = app.get_webview_window("toolbar") else {
        return;
    };
    let Ok(position) = window.outer_position() else {
        let _ = window.hide();
        return;
    };
    let Ok(size) = window.outer_size() else {
        let _ = window.hide();
        return;
    };

    let right = position.x.saturating_add(size.width as i32);
    let bottom = position.y.saturating_add(size.height as i32);
    let inside = point.x >= position.x
        && point.x < right
        && point.y >= position.y
        && point.y < bottom;
    if !inside {
        let _ = window.hide();
    }
}

fn should_hide_for_key(vk_code: u32) -> bool {
    !matches!(vk_code, 16 | 17 | 18 | 160 | 161 | 162 | 163 | 164 | 165)
}

#[cfg(windows)]
fn windows_double_click_time() -> u32 {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() }
}

#[cfg(not(windows))]
fn windows_double_click_time() -> u32 { 500 }
