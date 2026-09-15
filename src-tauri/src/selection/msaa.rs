use std::ffi::c_void;

use windows::{
    core::Interface,
    Win32::{
        Foundation::HWND,
        System::Variant::{VARIANT, VT_ARRAY, VT_BSTR, VT_DISPATCH, VT_I4, VT_UNKNOWN},
        UI::Accessibility::{AccessibleObjectFromWindow, IAccessible},
    },
};

const OBJID_CLIENT: u32 = 0xFFFF_FFFC;
const ROLE_SYSTEM_TEXT: i32 = 0x2A;

#[derive(Debug, PartialEq, Eq)]
pub enum MsaaSelection {
    Text(String),
    NonTextSelection,
    Unavailable,
}

pub fn selected_text(raw_hwnd: isize) -> MsaaSelection {
    let Some(accessible) = accessible_from_window(raw_hwnd) else {
        return MsaaSelection::Unavailable;
    };

    let Ok(selection) = (unsafe { accessible.accSelection() }) else {
        return MsaaSelection::Unavailable;
    };

    classify_selection(&accessible, &selection)
}

fn accessible_from_window(raw_hwnd: isize) -> Option<IAccessible> {
    if raw_hwnd == 0 {
        return None;
    }

    let hwnd = HWND(raw_hwnd as *mut c_void);
    let mut raw: *mut c_void = std::ptr::null_mut();
    unsafe {
        AccessibleObjectFromWindow(hwnd, OBJID_CLIENT, &IAccessible::IID, &mut raw).ok()?;
        (!raw.is_null()).then(|| IAccessible::from_raw(raw))
    }
}

fn classify_selection(accessible: &IAccessible, selection: &VARIANT) -> MsaaSelection {
    let vt = variant_type(selection);

    if vt == VT_BSTR.0 {
        let text = unsafe {
            let value = &*selection.Anonymous.Anonymous;
            (*value.Anonymous.bstrVal).to_string()
        };
        let cleaned = text.replace('\u{fffc}', "");
        return if cleaned.trim().is_empty() {
            MsaaSelection::Unavailable
        } else {
            MsaaSelection::Text(cleaned)
        };
    }

    if vt & VT_ARRAY.0 != 0 || vt == VT_UNKNOWN.0 || vt == VT_DISPATCH.0 {
        return MsaaSelection::NonTextSelection;
    }

    if vt == VT_I4.0 {
        if let Ok(role) = unsafe { accessible.get_accRole(selection) } {
            if variant_type(&role) == VT_I4.0 {
                let role_value = unsafe {
                    let value = &*role.Anonymous.Anonymous;
                    value.Anonymous.lVal
                };
                if role_value != ROLE_SYSTEM_TEXT {
                    return MsaaSelection::NonTextSelection;
                }
            }
        }
    }

    MsaaSelection::Unavailable
}

fn variant_type(value: &VARIANT) -> u16 {
    unsafe { (*value.Anonymous.Anonymous).vt.0 }
}
