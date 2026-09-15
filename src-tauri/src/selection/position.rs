use windows::Win32::{
    Foundation::{POINT as WinPoint, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
};

use super::types::{DetectionType, Point, SelectionCandidate};

const TEXT_GAP: i32 = 6;
const MOUSE_GAP: i32 = 16;
const LINE_DIRECTION_THRESHOLD: i32 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkArea {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

pub fn toolbar_position(candidate: SelectionCandidate, width: i32, height: i32) -> Point {
    let desired = desired_position(candidate, width, height);
    match work_area_at(candidate.mouse_end) {
        Some(work_area) => clamp_to_work_area(desired, width, height, work_area),
        None => desired,
    }
}

pub fn result_position(anchor: Point, width: i32, height: i32) -> Point {
    let Some(work_area) = work_area_at(anchor) else {
        return Point {
            x: anchor.x - width / 2,
            y: anchor.y + 20,
        };
    };

    let below_y = anchor.y + 20;
    let above_y = anchor.y - height - 20;
    let y = if below_y + height <= work_area.bottom {
        below_y
    } else {
        above_y
    };

    clamp_to_work_area(
        Point {
            x: anchor.x - width / 2,
            y,
        },
        width,
        height,
        work_area,
    )
}

fn desired_position(candidate: SelectionCandidate, width: i32, height: i32) -> Point {
    let start = candidate.mouse_start;
    let end = candidate.mouse_end;

    match candidate.detection {
        DetectionType::DoubleClick | DetectionType::ShiftClick => Point {
            x: end.x - width / 2,
            y: end.y + TEXT_GAP,
        },
        DetectionType::Drag => {
            let dx = end.x - start.x;
            let dy = end.y - start.y;

            if dy.abs() > LINE_DIRECTION_THRESHOLD {
                if dy > 0 {
                    Point {
                        x: end.x,
                        y: end.y + MOUSE_GAP,
                    }
                } else {
                    Point {
                        x: end.x,
                        y: end.y - height - MOUSE_GAP,
                    }
                }
            } else if dx >= 0 {
                Point {
                    x: end.x,
                    y: start.y.max(end.y) + MOUSE_GAP,
                }
            } else {
                Point {
                    x: end.x - width,
                    y: start.y.max(end.y) + MOUSE_GAP,
                }
            }
        }
    }
}

fn clamp_to_work_area(mut point: Point, width: i32, height: i32, area: WorkArea) -> Point {
    let max_x = (area.right - width).max(area.left);
    let max_y = (area.bottom - height).max(area.top);
    point.x = point.x.clamp(area.left, max_x);
    point.y = point.y.clamp(area.top, max_y);
    point
}

fn work_area_at(point: Point) -> Option<WorkArea> {
    let monitor = unsafe {
        MonitorFromPoint(
            WinPoint {
                x: point.x,
                y: point.y,
            },
            MONITOR_DEFAULTTONEAREST,
        )
    };
    if monitor.is_invalid() {
        return None;
    }

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }

    Some(work_area_from_rect(info.rcWork))
}

fn work_area_from_rect(rect: RECT) -> WorkArea {
    WorkArea {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::types::SelectionCandidate;

    fn candidate(detection: DetectionType, start: Point, end: Point) -> SelectionCandidate {
        SelectionCandidate {
            detection,
            hwnd: 1,
            mouse_start: start,
            mouse_end: end,
            mouse_down_cursor: 0,
            mouse_up_cursor: 0,
        }
    }

    #[test]
    fn right_drag_places_toolbar_after_selection() {
        let pos = desired_position(
            candidate(
                DetectionType::Drag,
                Point { x: 100, y: 100 },
                Point { x: 220, y: 102 },
            ),
            350,
            43,
        );
        assert_eq!(pos, Point { x: 220, y: 118 });
    }

    #[test]
    fn upward_drag_places_toolbar_above_endpoint() {
        let pos = desired_position(
            candidate(
                DetectionType::Drag,
                Point { x: 300, y: 400 },
                Point { x: 280, y: 200 },
            ),
            350,
            43,
        );
        assert_eq!(pos, Point { x: 280, y: 141 });
    }

    #[test]
    fn clamp_keeps_toolbar_inside_negative_coordinate_monitor() {
        let area = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let pos = clamp_to_work_area(Point { x: -100, y: 1030 }, 350, 43, area);
        assert_eq!(pos, Point { x: -350, y: 997 });
    }

    #[test]
    fn result_prefers_above_when_bottom_has_no_room() {
        let area = WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        let anchor = Point { x: 1000, y: 950 };
        let below_y = anchor.y + 20;
        let above_y = anchor.y - 360 - 20;
        let y = if below_y + 360 <= area.bottom { below_y } else { above_y };
        let pos = clamp_to_work_area(Point { x: anchor.x - 260, y }, 520, 360, area);
        assert_eq!(pos, Point { x: 740, y: 570 });
    }
}
