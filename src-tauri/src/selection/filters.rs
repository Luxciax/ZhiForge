use crate::domain::app_rule::{AppRule, AppRuleBehavior, ClipboardFallbackPolicy};

const DEFAULT_WINDOWS_BLACKLIST: &[&str] = &[
    "explorer.exe",
    "snipaste.exe",
    "pixpin.exe",
    "sharex.exe",
    "excel.exe",
    "powerpnt.exe",
    "photoshop.exe",
    "illustrator.exe",
    "adobe premiere pro.exe",
    "afterfx.exe",
    "adobe audition.exe",
    "blender.exe",
    "3dsmax.exe",
    "maya.exe",
    "acad.exe",
    "sldworks.exe",
    "mstsc.exe",
];

pub fn is_blocked(program_name: &str, rules: &[AppRule]) -> bool {
    let program_name = program_name.to_ascii_lowercase();

    if let Some(behavior) = rules.iter().rev().find_map(|rule| {
        let pattern = rule.process_pattern.trim().to_ascii_lowercase();
        (!pattern.is_empty()
            && !pattern.contains('*')
            && pattern == program_name
            && rule.behavior != AppRuleBehavior::Inherit)
            .then_some(rule.behavior)
    }) {
        return matches!(behavior, AppRuleBehavior::Disable);
    }

    if let Some(behavior) = rules.iter().rev().find_map(|rule| {
        let pattern = rule.process_pattern.trim().to_ascii_lowercase();
        (!pattern.is_empty()
            && pattern.contains('*')
            && wildcard_match(&pattern, &program_name)
            && rule.behavior != AppRuleBehavior::Inherit)
            .then_some(rule.behavior)
    }) {
        return matches!(behavior, AppRuleBehavior::Disable);
    }

    is_default_blocked(&program_name)
}

pub fn is_default_blocked(program_name: &str) -> bool {
    let program_name = program_name.to_ascii_lowercase();
    DEFAULT_WINDOWS_BLACKLIST
        .iter()
        .any(|blocked| program_name.contains(blocked))
}

pub fn clipboard_fallback_policy(program_name: &str, rules: &[AppRule]) -> ClipboardFallbackPolicy {
    let program_name = program_name.to_ascii_lowercase();

    if let Some(policy) = rules.iter().rev().find_map(|rule| {
        let pattern = rule.process_pattern.trim().to_ascii_lowercase();
        (!pattern.is_empty()
            && !pattern.contains('*')
            && pattern == program_name
            && rule.clipboard_fallback != ClipboardFallbackPolicy::Inherit)
            .then_some(rule.clipboard_fallback)
    }) {
        return policy;
    }

    if let Some(policy) = rules.iter().rev().find_map(|rule| {
        let pattern = rule.process_pattern.trim().to_ascii_lowercase();
        (!pattern.is_empty()
            && pattern.contains('*')
            && wildcard_match(&pattern, &program_name)
            && rule.clipboard_fallback != ClipboardFallbackPolicy::Inherit)
            .then_some(rule.clipboard_fallback)
    }) {
        return policy;
    }

    ClipboardFallbackPolicy::Inherit
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut cursor = 0usize;
    let starts_wild = pattern.starts_with('*');
    let ends_wild = pattern.ends_with('*');

    for (index, part) in parts.iter().filter(|part| !part.is_empty()).enumerate() {
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && !starts_wild && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }

    ends_wild || parts.last().map(|part| value.ends_with(part)).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::app_rule::ClipboardFallbackPolicy;

    fn rule(pattern: &str, behavior: AppRuleBehavior) -> AppRule {
        AppRule {
            id: pattern.into(),
            process_pattern: pattern.into(),
            behavior,
            clipboard_fallback: ClipboardFallbackPolicy::Inherit,
        }
    }

    #[test]
    fn blacklist_is_case_insensitive() {
        assert!(is_default_blocked("C:\\Windows\\EXPLORER.EXE"));
        assert!(!is_default_blocked("chrome.exe"));
    }

    #[test]
    fn exact_user_rule_can_enable_default_blocked_app() {
        assert!(!is_blocked("excel.exe", &[rule("excel.exe", AppRuleBehavior::Enable)]));
    }

    #[test]
    fn wildcard_rule_can_disable_app_family() {
        assert!(is_blocked("chrome.exe", &[rule("*chrome*", AppRuleBehavior::Disable)]));
    }

    #[test]
    fn inherit_rule_falls_back_to_builtin_default() {
        assert!(is_blocked("excel.exe", &[rule("excel.exe", AppRuleBehavior::Inherit)]));
    }

    #[test]
    fn exact_clipboard_policy_overrides_wildcard() {
        let mut wildcard = rule("*code*", AppRuleBehavior::Inherit);
        wildcard.clipboard_fallback = ClipboardFallbackPolicy::Deny;
        let mut exact = rule("code.exe", AppRuleBehavior::Inherit);
        exact.clipboard_fallback = ClipboardFallbackPolicy::Allow;
        assert_eq!(
            clipboard_fallback_policy("CODE.EXE", &[wildcard, exact]),
            ClipboardFallbackPolicy::Allow
        );
    }
}
