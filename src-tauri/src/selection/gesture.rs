use crate::domain::trigger::{RequiredModifier, TriggerProfile};

use super::types::{DetectionType, InputEvent, MouseSnapshot, SelectionCandidate};

const DOUBLE_CLICK_MAX_DISTANCE: i64 = 3;

#[derive(Debug)]
pub struct GestureEngine {
    double_click_time_ms: u32,
    last_down: Option<MouseSnapshot>,
    last_up: Option<MouseSnapshot>,
    last_click_valid: bool,
}

impl GestureEngine {
    pub fn new(double_click_time_ms: u32) -> Self {
        Self {
            double_click_time_ms,
            last_down: None,
            last_up: None,
            last_click_valid: false,
        }
    }

    pub fn reset(&mut self) {
        self.last_down = None;
        self.last_up = None;
        self.last_click_valid = false;
    }

    pub fn handle(
        &mut self,
        event: &InputEvent,
        profile: &TriggerProfile,
    ) -> Option<SelectionCandidate> {
        if !profile.selection_enabled {
            self.reset();
            return None;
        }

        match event {
            InputEvent::LeftDown(snapshot) => {
                self.last_down = Some(*snapshot);
                None
            }
            InputEvent::LeftUp(up) => self.handle_left_up(*up, profile),
            _ => None,
        }
    }

    fn handle_left_up(
        &mut self,
        up: MouseSnapshot,
        profile: &TriggerProfile,
    ) -> Option<SelectionCandidate> {
        let down = self.last_down?;
        let elapsed = up.timestamp_ms.wrapping_sub(down.timestamp_ms);
        let distance_sq = distance_sq(down, up);
        let same_window = down.hwnd != 0 && down.hwnd == up.hwnd && down.window_rect == up.window_rect;
        let current_click_valid = elapsed <= self.double_click_time_ms;
        let modifier_matches = required_modifier_matches(up.modifiers, profile.required_modifier);
        let min_drag_distance = i64::from(profile.min_drag_distance);

        let detection = if profile.drag_enabled
            && modifier_matches
            && elapsed <= profile.max_drag_duration_ms
            && distance_sq >= min_drag_distance * min_drag_distance
            && same_window
        {
            Some(DetectionType::Drag)
        } else if profile.double_click_enabled
            && modifier_matches
            && self.last_click_valid
            && current_click_valid
            && same_window
        {
            self.last_up.and_then(|previous_up| {
                let between_clicks = down.timestamp_ms.wrapping_sub(previous_up.timestamp_ms);
                let near_previous = point_distance_sq(previous_up.point.x, previous_up.point.y, up.point.x, up.point.y)
                    <= DOUBLE_CLICK_MAX_DISTANCE * DOUBLE_CLICK_MAX_DISTANCE;
                let tiny_second_click = distance_sq <= DOUBLE_CLICK_MAX_DISTANCE * DOUBLE_CLICK_MAX_DISTANCE;
                (between_clicks <= self.double_click_time_ms && near_previous && tiny_second_click)
                    .then_some(DetectionType::DoubleClick)
            })
        } else if profile.shift_click_enabled
            && matches!(profile.required_modifier, RequiredModifier::None | RequiredModifier::Shift)
            && elapsed <= profile.max_drag_duration_ms
            && up.modifiers & 0b001 != 0
            && up.modifiers & 0b110 == 0
        {
            Some(DetectionType::ShiftClick)
        } else {
            None
        };

        self.last_click_valid = current_click_valid;
        self.last_up = Some(up);

        detection.map(|detection| SelectionCandidate {
            detection,
            hwnd: up.hwnd,
            mouse_start: down.point,
            mouse_end: up.point,
            mouse_down_cursor: down.cursor,
            mouse_up_cursor: up.cursor,
        })
    }
}

fn required_modifier_matches(mask: u8, required: RequiredModifier) -> bool {
    match required {
        RequiredModifier::None => true,
        RequiredModifier::Shift => mask & 0b001 != 0,
        RequiredModifier::Ctrl => mask & 0b010 != 0,
        RequiredModifier::Alt => mask & 0b100 != 0,
    }
}

fn distance_sq(a: MouseSnapshot, b: MouseSnapshot) -> i64 {
    point_distance_sq(a.point.x, a.point.y, b.point.x, b.point.y)
}

fn point_distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i64 {
    let dx = i64::from(bx - ax);
    let dy = i64::from(by - ay);
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::types::{Point, Rect};

    fn snap(x: i32, y: i32, time: u32, modifiers: u8) -> MouseSnapshot {
        MouseSnapshot {
            point: Point { x, y },
            timestamp_ms: time,
            modifiers,
            hwnd: 1,
            cursor: 0,
            window_rect: Rect { left: 0, top: 0, right: 800, bottom: 600 },
        }
    }

    #[test]
    fn drag_requires_eight_pixels_by_default() {
        let mut engine = GestureEngine::new(500);
        let profile = TriggerProfile::default();
        engine.handle(&InputEvent::LeftDown(snap(10, 10, 100, 0)), &profile);
        let result = engine.handle(&InputEvent::LeftUp(snap(18, 10, 200, 0)), &profile).unwrap();
        assert_eq!(result.detection, DetectionType::Drag);
    }

    #[test]
    fn tiny_single_click_does_not_trigger() {
        let mut engine = GestureEngine::new(500);
        let profile = TriggerProfile::default();
        engine.handle(&InputEvent::LeftDown(snap(10, 10, 100, 0)), &profile);
        assert!(engine.handle(&InputEvent::LeftUp(snap(11, 10, 150, 0)), &profile).is_none());
    }

    #[test]
    fn shift_click_triggers_without_ctrl_or_alt() {
        let mut engine = GestureEngine::new(500);
        let profile = TriggerProfile::default();
        engine.handle(&InputEvent::LeftDown(snap(10, 10, 100, 0)), &profile);
        let result = engine.handle(&InputEvent::LeftUp(snap(11, 10, 200, 0b001)), &profile).unwrap();
        assert_eq!(result.detection, DetectionType::ShiftClick);
    }

    #[test]
    fn disabled_drag_does_not_trigger() {
        let mut engine = GestureEngine::new(500);
        let profile = TriggerProfile { drag_enabled: false, ..TriggerProfile::default() };
        engine.handle(&InputEvent::LeftDown(snap(10, 10, 100, 0)), &profile);
        assert!(engine.handle(&InputEvent::LeftUp(snap(40, 10, 200, 0)), &profile).is_none());
    }

    #[test]
    fn ctrl_requirement_blocks_plain_drag() {
        let mut engine = GestureEngine::new(500);
        let profile = TriggerProfile {
            required_modifier: RequiredModifier::Ctrl,
            ..TriggerProfile::default()
        };
        engine.handle(&InputEvent::LeftDown(snap(10, 10, 100, 0)), &profile);
        assert!(engine.handle(&InputEvent::LeftUp(snap(30, 10, 200, 0)), &profile).is_none());
        engine.handle(&InputEvent::LeftDown(snap(10, 10, 300, 0b010)), &profile);
        assert_eq!(
            engine.handle(&InputEvent::LeftUp(snap(30, 10, 400, 0b010)), &profile).unwrap().detection,
            DetectionType::Drag
        );
    }
}
