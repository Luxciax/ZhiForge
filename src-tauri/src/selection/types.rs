use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectionType {
    Drag,
    DoubleClick,
    ShiftClick,
}

#[derive(Clone, Copy, Debug)]
pub struct MouseSnapshot {
    pub point: Point,
    pub timestamp_ms: u32,
    pub modifiers: u8,
    pub hwnd: isize,
    pub cursor: isize,
    pub window_rect: Rect,
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    LeftDown(MouseSnapshot),
    LeftUp(MouseSnapshot),
    MouseWheel,
    KeyDown { vk_code: u32 },
}

#[derive(Clone, Copy, Debug)]
pub struct SelectionCandidate {
    pub detection: DetectionType,
    pub hwnd: isize,
    pub mouse_start: Point,
    pub mouse_end: Point,
    pub mouse_down_cursor: isize,
    pub mouse_up_cursor: isize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionPayload {
    pub text: String,
    pub program_name: String,
    pub method: &'static str,
    pub mouse_x: i32,
    pub mouse_y: i32,
}
