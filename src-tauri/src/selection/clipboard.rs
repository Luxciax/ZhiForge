use std::{
    mem::transmute,
    slice,
    thread::sleep,
    time::Duration,
};

use uiautomation::inputs::Keyboard;
use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL},
    Graphics::Gdi::{DeleteEnhMetaFile, GetEnhMetaFileBits, SetEnhMetaFileBits, HENHMETAFILE},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
            GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
    },
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
        WindowsAndMessaging::{LoadCursorW, IDC_IBEAM},
    },
};

use crate::domain::app_rule::ClipboardFallbackPolicy;

use super::types::SelectionCandidate;

const POLL_INTERVAL_MS: u64 = 5;
const CTRL_INSERT_POLLS: usize = 20;
const CTRL_C_POLLS: usize = 36;

const CF_TEXT: u32 = 1;
const CF_BITMAP: u32 = 2;
const CF_METAFILEPICT: u32 = 3;
const CF_OEMTEXT: u32 = 7;
const CF_PALETTE: u32 = 9;
const CF_UNICODETEXT: u32 = 13;
const CF_ENHMETAFILE: u32 = 14;
const CF_LOCALE: u32 = 16;
const CF_OWNERDISPLAY: u32 = 128;
const CF_DSPTEXT: u32 = 129;
const CF_DSPBITMAP: u32 = 130;
const CF_DSPMETAFILEPICT: u32 = 131;
const CF_DSPENHMETAFILE: u32 = 142;
const CF_PRIVATEFIRST: u32 = 512;
const CF_PRIVATELAST: u32 = 767;
const CF_GDIOBJFIRST: u32 = 768;
const CF_GDIOBJLAST: u32 = 1023;

const CLIPBOARD_CURSOR_BYPASS: &[&str] = &[
    "acrobat.exe",
    "acrord32.exe",
    "wps.exe",
    "cajviewer.exe",
    "foxitphantom.exe",
    "foxitreader.exe",
];
const CLIPBOARD_DELAY_READ: &[&str] = &[
    "acrobat.exe",
    "acrord32.exe",
    "wps.exe",
    "cajviewer.exe",
    "foxitphantom.exe",
    "foxitreader.exe",
    "winword.exe",
];

pub fn write_text(text: &str) -> Result<(), String> {
    let _open = ClipboardOpenGuard::open().ok_or_else(|| "clipboard is busy".to_string())?;
    unsafe { EmptyClipboard() }.map_err(|error| error.to_string())?;

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * std::mem::size_of::<u16>();
    let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) }.map_err(|error| error.to_string())?;
    let ptr = unsafe { GlobalLock(global) };
    if ptr.is_null() {
        let _ = unsafe { GlobalFree(Some(global)) };
        return Err("failed to lock clipboard memory".to_string());
    }

    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, byte_len);
    }
    let _ = unsafe { GlobalUnlock(global) };

    let handle: HANDLE = unsafe { transmute(global) };
    match unsafe { SetClipboardData(CF_UNICODETEXT, Some(handle)) } {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = unsafe { GlobalFree(Some(global)) };
            Err(error.to_string())
        }
    }
}

pub fn selected_text(
    program_name: &str,
    candidate: SelectionCandidate,
    policy: ClipboardFallbackPolicy,
) -> Option<String> {
    if user_modifier_active() || policy == ClipboardFallbackPolicy::Deny {
        return None;
    }
    if policy != ClipboardFallbackPolicy::Allow && !clipboard_context_allowed(program_name, candidate) {
        return None;
    }

    let delay_read = contains_program(CLIPBOARD_DELAY_READ, program_name);
    let backup = backup_and_clear()?;
    let restore_guard = ClipboardRestoreGuard::new(backup);

    if !delay_read {
        if let Some(text) = try_copy("{ctrl}{insert}", CTRL_INSERT_POLLS, 0) {
            drop(restore_guard);
            return Some(text);
        }
    }

    if user_modifier_active() {
        return None;
    }

    let settle_ms = if delay_read { 135 } else { 0 };
    let text = try_copy("{ctrl}c", CTRL_C_POLLS, settle_ms);
    drop(restore_guard);
    text
}

fn clipboard_context_allowed(program_name: &str, candidate: SelectionCandidate) -> bool {
    if contains_program(CLIPBOARD_CURSOR_BYPASS, program_name) {
        return true;
    }

    let Ok(ibeam) = (unsafe { LoadCursorW(None, IDC_IBEAM) }) else {
        return false;
    };
    let ibeam = ibeam.0 as isize;
    candidate.mouse_down_cursor == ibeam || candidate.mouse_up_cursor == ibeam
}

fn contains_program(list: &[&str], program_name: &str) -> bool {
    let program_name = program_name.to_ascii_lowercase();
    list.iter().any(|item| program_name.contains(item))
}

fn backup_and_clear() -> Option<ClipboardBackup> {
    let _open = ClipboardOpenGuard::open()?;
    let mut entries = Vec::new();
    let mut format = 0u32;

    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if is_skipped_format(format) {
            continue;
        }

        if format == CF_ENHMETAFILE {
            if let Some(entry) = backup_enhmetafile(format) {
                entries.push(entry);
            }
            continue;
        }

        if let Some(entry) = backup_global(format) {
            entries.push(entry);
        }
    }

    unsafe { EmptyClipboard().ok()?; }
    Some(ClipboardBackup { entries })
}

fn backup_global(format: u32) -> Option<ClipboardEntry> {
    let handle = unsafe { GetClipboardData(format).ok()? };
    if handle.is_invalid() {
        return None;
    }

    let global: HGLOBAL = unsafe { transmute(handle) };
    let size = unsafe { GlobalSize(global) };
    if size == 0 {
        return None;
    }

    let ptr = unsafe { GlobalLock(global) };
    if ptr.is_null() {
        return None;
    }

    let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, size).to_vec() };
    let _ = unsafe { GlobalUnlock(global) };
    Some(ClipboardEntry {
        format,
        kind: ClipboardEntryKind::Global,
        bytes,
    })
}

fn backup_enhmetafile(format: u32) -> Option<ClipboardEntry> {
    let handle = unsafe { GetClipboardData(format).ok()? };
    if handle.is_invalid() {
        return None;
    }

    let metafile: HENHMETAFILE = unsafe { transmute(handle) };
    let size = unsafe { GetEnhMetaFileBits(metafile, None) } as usize;
    if size == 0 {
        return None;
    }

    let mut bytes = vec![0u8; size];
    let written = unsafe { GetEnhMetaFileBits(metafile, Some(&mut bytes)) } as usize;
    if written == 0 {
        return None;
    }
    bytes.truncate(written);

    Some(ClipboardEntry {
        format,
        kind: ClipboardEntryKind::EnhMetaFile,
        bytes,
    })
}

fn restore_backup(backup: ClipboardBackup) -> bool {
    let Some(_open) = ClipboardOpenGuard::open() else {
        return false;
    };
    if unsafe { EmptyClipboard() }.is_err() {
        return false;
    }

    let mut all_ok = true;
    for entry in backup.entries {
        let ok = match entry.kind {
            ClipboardEntryKind::Global => restore_global(entry.format, &entry.bytes),
            ClipboardEntryKind::EnhMetaFile => restore_enhmetafile(entry.format, &entry.bytes),
        };
        all_ok &= ok;
    }
    all_ok
}

fn restore_global(format: u32, bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let Ok(global) = (unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }) else {
        return false;
    };
    let ptr = unsafe { GlobalLock(global) };
    if ptr.is_null() {
        let _ = unsafe { GlobalFree(Some(global)) };
        return false;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }
    let _ = unsafe { GlobalUnlock(global) };

    let handle: HANDLE = unsafe { transmute(global) };
    match unsafe { SetClipboardData(format, Some(handle)) } {
        Ok(_) => true,
        Err(_) => {
            let _ = unsafe { GlobalFree(Some(global)) };
            false
        }
    }
}

fn restore_enhmetafile(format: u32, bytes: &[u8]) -> bool {
    let metafile = unsafe { SetEnhMetaFileBits(bytes) };
    if metafile.0.is_null() {
        return false;
    }

    let handle: HANDLE = unsafe { transmute(metafile) };
    if unsafe { SetClipboardData(format, Some(handle)) }.is_ok() {
        true
    } else {
        let _ = unsafe { DeleteEnhMetaFile(Some(metafile)) };
        false
    }
}

fn try_copy(keys: &str, polls: usize, settle_ms: u64) -> Option<String> {
    let before = unsafe { GetClipboardSequenceNumber() };
    Keyboard::new().interval(0).send_keys(keys).ok()?;

    for _ in 0..polls {
        sleep(Duration::from_millis(POLL_INTERVAL_MS));
        if unsafe { GetClipboardSequenceNumber() } != before {
            if settle_ms > 0 {
                sleep(Duration::from_millis(settle_ms));
            }
            sleep(Duration::from_millis(10));
            return read_unicode_text();
        }
    }
    None
}

fn read_unicode_text() -> Option<String> {
    let _open = ClipboardOpenGuard::open()?;
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT).ok()? };
    if handle.is_invalid() {
        return None;
    }

    let global: HGLOBAL = unsafe { transmute(handle) };
    let size = unsafe { GlobalSize(global) };
    if size < 2 {
        return None;
    }

    let ptr = unsafe { GlobalLock(global) } as *const u16;
    if ptr.is_null() {
        return None;
    }

    let units = unsafe { slice::from_raw_parts(ptr, size / 2) };
    let len = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
    let text = String::from_utf16_lossy(&units[..len]).replace('\u{fffc}', "");
    let _ = unsafe { GlobalUnlock(global) };

    (!text.trim().is_empty()).then_some(text)
}

fn user_modifier_active() -> bool {
    unsafe {
        GetAsyncKeyState(VK_CONTROL.0 as i32) < 0
            || GetAsyncKeyState(VK_MENU.0 as i32) < 0
            || GetAsyncKeyState(VK_SHIFT.0 as i32) < 0
    }
}

fn is_skipped_format(format: u32) -> bool {
    matches!(
        format,
        CF_TEXT
            | CF_OEMTEXT
            | CF_LOCALE
            | CF_BITMAP
            | CF_PALETTE
            | CF_METAFILEPICT
            | CF_OWNERDISPLAY
            | CF_DSPTEXT
            | CF_DSPBITMAP
            | CF_DSPMETAFILEPICT
            | CF_DSPENHMETAFILE
    ) || (CF_PRIVATEFIRST..=CF_PRIVATELAST).contains(&format)
        || (CF_GDIOBJFIRST..=CF_GDIOBJLAST).contains(&format)
}

struct ClipboardOpenGuard;

impl ClipboardOpenGuard {
    fn open() -> Option<Self> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Some(Self);
            }
            sleep(Duration::from_millis(5));
        }
        None
    }
}

impl Drop for ClipboardOpenGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

struct ClipboardBackup {
    entries: Vec<ClipboardEntry>,
}

struct ClipboardEntry {
    format: u32,
    kind: ClipboardEntryKind,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
enum ClipboardEntryKind {
    Global,
    EnhMetaFile,
}

struct ClipboardRestoreGuard {
    backup: Option<ClipboardBackup>,
}

impl ClipboardRestoreGuard {
    fn new(backup: ClipboardBackup) -> Self {
        Self { backup: Some(backup) }
    }
}

impl Drop for ClipboardRestoreGuard {
    fn drop(&mut self) {
        if let Some(backup) = self.backup.take() {
            let _ = restore_backup(backup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_lists_are_case_insensitive() {
        assert!(contains_program(CLIPBOARD_CURSOR_BYPASS, "C:\\Apps\\Acrobat.EXE"));
        assert!(contains_program(CLIPBOARD_CURSOR_BYPASS, "AcroRd32.exe"));
        assert!(contains_program(CLIPBOARD_DELAY_READ, "FOXITPHANTOM.EXE"));
        assert!(contains_program(CLIPBOARD_DELAY_READ, "WINWORD.EXE"));
        assert!(!contains_program(CLIPBOARD_CURSOR_BYPASS, "chrome.exe"));
    }

    #[test]
    fn copied_files_are_backupable() {
        const CF_HDROP: u32 = 15;
        assert!(!is_skipped_format(CF_HDROP));
        assert!(is_skipped_format(CF_TEXT));
        assert!(is_skipped_format(CF_PRIVATEFIRST));
        assert!(!is_skipped_format(CF_ENHMETAFILE));
    }
}
