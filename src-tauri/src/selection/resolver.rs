use std::ffi::c_void;

use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{CloseHandle, HWND},
        System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        UI::WindowsAndMessaging::GetWindowThreadProcessId,
    },
};

use crate::domain::app_rule::AppRule;

use super::{
    clipboard,
    filters,
    msaa::{self, MsaaSelection},
    types::{SelectionCandidate, SelectionPayload},
    uia,
};

pub fn resolve(candidate: SelectionCandidate, app_rules: &[AppRule]) -> Option<SelectionPayload> {
    let program_name = program_name(candidate.hwnd).unwrap_or_default();
    if filters::is_blocked(&program_name, app_rules) {
        return None;
    }

    if let Some(text) = uia::selected_text() {
        return Some(payload(candidate, program_name, text, "uia"));
    }

    match msaa::selected_text(candidate.hwnd) {
        MsaaSelection::Text(text) => return Some(payload(candidate, program_name, text, "accessibility")),
        MsaaSelection::NonTextSelection => return None,
        MsaaSelection::Unavailable => {}
    }

    let clipboard_policy = filters::clipboard_fallback_policy(&program_name, app_rules);
    let text = clipboard::selected_text(&program_name, candidate, clipboard_policy)?;
    Some(payload(candidate, program_name, text, "clipboard"))
}

fn payload(
    candidate: SelectionCandidate,
    program_name: String,
    text: String,
    method: &'static str,
) -> SelectionPayload {
    SelectionPayload {
        text,
        program_name,
        method,
        mouse_x: candidate.mouse_end.x,
        mouse_y: candidate.mouse_end.y,
    }
}

fn program_name(raw_hwnd: isize) -> Option<String> {
    if raw_hwnd == 0 { return None; }
    let hwnd = HWND(raw_hwnd as *mut c_void);
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)); }
    if pid == 0 { return None; }

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()? };
    let mut buffer = vec![0u16; 1024];
    let mut len = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len)
    };
    unsafe { let _ = CloseHandle(process); }
    result.ok()?;

    let path = String::from_utf16_lossy(&buffer[..len as usize]);
    Some(path.rsplit(['\\', '/']).next().unwrap_or(&path).to_ascii_lowercase())
}
