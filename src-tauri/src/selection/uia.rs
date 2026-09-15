use uiautomation::{patterns::UITextPattern, UIAutomation, UIElement};

pub fn selected_text() -> Option<String> {
    let automation = UIAutomation::new().ok()?;
    let focused = automation.get_focused_element().ok()?;

    if let Some(text) = text_from_element(&focused) {
        return Some(text);
    }

    let walker = automation.get_control_view_walker().ok()?;
    let mut current = focused;
    for _ in 0..10 {
        current = walker.get_parent(&current).ok()?;
        if let Some(text) = text_from_element(&current) {
            return Some(text);
        }
    }
    None
}

fn text_from_element(element: &UIElement) -> Option<String> {
    let pattern = element.get_pattern::<UITextPattern>().ok()?;
    let ranges = pattern.get_selection().ok()?;
    ranges.into_iter().find_map(|range| {
        let text = range.get_text(-1).ok()?;
        let cleaned = clean_text(text);
        (!cleaned.trim().is_empty()).then_some(cleaned)
    })
}

fn clean_text(text: String) -> String {
    text.replace('\u{fffc}', "")
}
