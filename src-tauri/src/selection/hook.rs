use std::{
    sync::{mpsc::SyncSender, Mutex, OnceLock},
    thread::{self, JoinHandle},
};

use windows::Win32::{
    Foundation::{LPARAM, LRESULT, RECT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetCursorInfo, GetMessageW, GetWindowRect, PostThreadMessageW,
            SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WindowFromPoint, CURSORINFO, KBDLLHOOKSTRUCT,
            MSLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
            WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_QUIT, WM_SYSKEYDOWN,
        },
    },
};

use super::types::{InputEvent, MouseSnapshot, Point, Rect};

static EVENT_SENDER: OnceLock<Mutex<Option<SyncSender<InputEvent>>>> = OnceLock::new();

pub struct HookHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub fn start(sender: SyncSender<InputEvent>) -> Result<HookHandle, String> {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let join = thread::Builder::new()
        .name("selection-input-hook".into())
        .spawn(move || unsafe {
            *EVENT_SENDER.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(sender);
            let thread_id = GetCurrentThreadId();

            let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
            let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0);
            let (mouse, keyboard) = match (mouse, keyboard) {
                (Ok(mouse), Ok(keyboard)) => (mouse, keyboard),
                (mouse, keyboard) => {
                    if let Ok(hook) = mouse { let _ = UnhookWindowsHookEx(hook); }
                    if let Ok(hook) = keyboard { let _ = UnhookWindowsHookEx(hook); }
                    let _ = ready_tx.send(Err("failed to install global input hooks".to_string()));
                    *EVENT_SENDER.get().unwrap().lock().unwrap() = None;
                    return;
                }
            };

            let _ = ready_tx.send(Ok(thread_id));
            let mut message = MSG::default();
            loop {
                let status = GetMessageW(&mut message, None, 0, 0).0;
                if status <= 0 {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            let _ = UnhookWindowsHookEx(mouse);
            let _ = UnhookWindowsHookEx(keyboard);
            *EVENT_SENDER.get().unwrap().lock().unwrap() = None;
        })
        .map_err(|error| error.to_string())?;

    match ready_rx.recv().map_err(|error| error.to_string())? {
        Ok(thread_id) => Ok(HookHandle { thread_id, join: Some(join) }),
        Err(error) => {
            let _ = join.join();
            Err(error)
        }
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);

        let event = match message {
            WM_LBUTTONDOWN => Some(InputEvent::LeftDown(mouse_snapshot(info))),
            WM_LBUTTONUP => Some(InputEvent::LeftUp(mouse_snapshot(info))),
            WM_MOUSEWHEEL => Some(InputEvent::MouseWheel),
            _ => None,
        };

        if let Some(event) = event {
            send(event);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        send(InputEvent::KeyDown { vk_code: info.vkCode });
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe fn mouse_snapshot(info: &MSLLHOOKSTRUCT) -> MouseSnapshot {
    let hwnd = WindowFromPoint(info.pt);
    let mut raw_rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut raw_rect);
    let mut cursor_info = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    let cursor = GetCursorInfo(&mut cursor_info)
        .ok()
        .map(|_| cursor_info.hCursor.0 as isize)
        .unwrap_or_default();
    MouseSnapshot {
        point: Point { x: info.pt.x, y: info.pt.y },
        timestamp_ms: info.time,
        modifiers: modifier_mask(),
        hwnd: hwnd.0 as isize,
        cursor,
        window_rect: Rect {
            left: raw_rect.left,
            top: raw_rect.top,
            right: raw_rect.right,
            bottom: raw_rect.bottom,
        },
    }
}

unsafe fn modifier_mask() -> u8 {
    let mut mask = 0;
    if GetAsyncKeyState(VK_SHIFT.0 as i32) < 0 { mask |= 0b001; }
    if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 { mask |= 0b010; }
    if GetAsyncKeyState(VK_MENU.0 as i32) < 0 { mask |= 0b100; }
    mask
}

fn send(event: InputEvent) {
    if let Some(sender) = EVENT_SENDER.get().and_then(|slot| slot.lock().ok()).and_then(|guard| guard.clone()) {
        let _ = sender.try_send(event);
    }
}
