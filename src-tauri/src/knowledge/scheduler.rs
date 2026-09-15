const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const MIN_STABILITY_DAYS: f64 = 0.25;
const MAX_STABILITY_DAYS: f64 = 3650.0;
const MAX_SCHEDULE_DAYS: i64 = 365;

#[derive(Clone, Debug)]
pub(crate) struct SchedulerState {
    pub stability_days: f64,
    pub difficulty: f64,
    pub lapse_count: i64,
    pub scheduled_days: i64,
    pub review_count: i64,
    pub last_reviewed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub(crate) struct SchedulerDecision {
    pub stability_days: f64,
    pub difficulty: f64,
    pub lapse_count: i64,
    pub scheduled_days: i64,
    pub next_review_at: i64,
    pub elapsed_days: f64,
    pub retrievability: f64,
    pub last_result: &'static str,
}

pub(crate) fn schedule_review(
    state: &SchedulerState,
    result: &str,
    now: i64,
) -> Result<SchedulerDecision, String> {
    let stability = normalize_stability(state.stability_days);
    let difficulty = normalize_difficulty(state.difficulty);
    let elapsed_days = elapsed_days(state, now);
    let retrievability = retrievability(stability, elapsed_days);

    let (next_stability, next_difficulty, next_lapses, scheduled_days, last_result) = match result {
        "wrong" => {
            let next_stability = (stability * (0.25 + 0.20 * retrievability))
                .clamp(MIN_STABILITY_DAYS, MAX_STABILITY_DAYS);
            (
                next_stability,
                (difficulty + 0.8).clamp(1.0, 10.0),
                state.lapse_count.saturating_add(1),
                0,
                "wrong",
            )
        }
        "partial" => {
            let next_stability = (stability * (0.85 + 0.20 * (1.0 - retrievability)) + 0.5)
                .clamp(0.75, MAX_STABILITY_DAYS);
            let days = stability_to_days(next_stability, 0.75).max(1);
            (
                next_stability,
                (difficulty + 0.15).clamp(1.0, 10.0),
                state.lapse_count.max(0),
                days,
                "partial",
            )
        }
        "correct" => {
            let next_difficulty = (difficulty - 0.25).clamp(1.0, 10.0);
            let desirable_difficulty = (1.0 - retrievability).clamp(0.0, 0.70);
            let ease = ((10.5 - next_difficulty) / 10.0).clamp(0.05, 0.95);
            let growth = 1.35 + (0.55 * ease) + (0.65 * desirable_difficulty);
            let next_stability = (stability * growth)
                .max(stability + 0.5)
                .clamp(1.0, MAX_STABILITY_DAYS);
            let days = stability_to_days(next_stability, 1.0).max(1);
            (
                next_stability,
                next_difficulty,
                state.lapse_count.max(0),
                days,
                "correct",
            )
        }
        _ => return Err("unsupported judgement result".into()),
    };

    let delay_ms = scheduled_days.saturating_mul(DAY_MS);
    Ok(SchedulerDecision {
        stability_days: round_metric(next_stability),
        difficulty: round_metric(next_difficulty),
        lapse_count: next_lapses,
        scheduled_days,
        next_review_at: now.saturating_add(delay_ms),
        elapsed_days: round_metric(elapsed_days),
        retrievability: round_metric(retrievability),
        last_result,
    })
}

#[cfg(test)]
fn baseline_stability_days(mastery_score: i64) -> f64 {
    match mastery_score.clamp(0, 100) {
        0..=39 => 0.5,
        40..=69 => 1.0,
        70..=89 => 3.0,
        _ => 7.0,
    }
}

#[cfg(test)]
fn baseline_difficulty(mastery_score: i64, wrong_count: i64) -> f64 {
    let mastery_component = 8.0 - (mastery_score.clamp(0, 100) as f64 / 20.0);
    let lapse_component = (wrong_count.max(0) as f64 * 0.2).min(1.5);
    round_metric((mastery_component + lapse_component).clamp(1.0, 10.0))
}

fn elapsed_days(state: &SchedulerState, now: i64) -> f64 {
    match state.last_reviewed_at {
        Some(last) if now > last => ((now - last) as f64 / DAY_MS as f64).max(0.0),
        _ if state.review_count > 0 => state.scheduled_days.max(0) as f64,
        _ => 0.0,
    }
}

fn retrievability(stability_days: f64, elapsed_days: f64) -> f64 {
    if elapsed_days <= 0.0 {
        return 1.0;
    }
    0.9_f64
        .powf(elapsed_days / normalize_stability(stability_days))
        .clamp(0.0, 1.0)
}

fn stability_to_days(stability_days: f64, multiplier: f64) -> i64 {
    ((stability_days * multiplier).round() as i64).clamp(0, MAX_SCHEDULE_DAYS)
}

fn normalize_stability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(MIN_STABILITY_DAYS, MAX_STABILITY_DAYS)
    } else {
        1.0
    }
}

fn normalize_difficulty(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(1.0, 10.0)
    } else {
        5.0
    }
}

fn round_metric(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> SchedulerState {
        SchedulerState {
            stability_days: 3.0,
            difficulty: 5.0,
            lapse_count: 1,
            scheduled_days: 3,
            review_count: 4,
            last_reviewed_at: Some(1_000),
        }
    }

    #[test]
    fn correct_answer_increases_stability_and_interval() {
        let decision = schedule_review(&state(), "correct", 1_000 + 3 * DAY_MS).unwrap();
        assert!(decision.stability_days > 3.0);
        assert!(decision.difficulty < 5.0);
        assert!(decision.scheduled_days >= 4);
        assert_eq!(
            decision.next_review_at,
            1_000 + 3 * DAY_MS + decision.scheduled_days * DAY_MS
        );
        assert_eq!(decision.last_result, "correct");
        assert!((decision.elapsed_days - 3.0).abs() < 0.001);
        assert!(decision.retrievability > 0.0 && decision.retrievability < 1.0);
    }

    #[test]
    fn later_success_gets_more_growth_than_early_success() {
        let early = schedule_review(&state(), "correct", 1_000 + DAY_MS).unwrap();
        let late = schedule_review(&state(), "correct", 1_000 + 6 * DAY_MS).unwrap();
        assert!(late.stability_days > early.stability_days);
        assert!(late.scheduled_days >= early.scheduled_days);
    }

    #[test]
    fn wrong_answer_is_due_immediately_and_counts_a_lapse() {
        let decision = schedule_review(&state(), "wrong", 99_000).unwrap();
        assert!(decision.stability_days < 3.0);
        assert!(decision.difficulty > 5.0);
        assert_eq!(decision.lapse_count, 2);
        assert_eq!(decision.scheduled_days, 0);
        assert_eq!(decision.next_review_at, 99_000);
    }

    #[test]
    fn partial_answer_stays_conservative() {
        let partial = schedule_review(&state(), "partial", 1_000 + 3 * DAY_MS).unwrap();
        let correct = schedule_review(&state(), "correct", 1_000 + 3 * DAY_MS).unwrap();
        assert!(partial.scheduled_days >= 1);
        assert!(partial.scheduled_days < correct.scheduled_days);
        assert!(partial.difficulty > 5.0);
    }

    #[test]
    fn baseline_uses_mastery_and_lapses_without_inventing_history() {
        assert_eq!(baseline_stability_days(20), 0.5);
        assert_eq!(baseline_stability_days(80), 3.0);
        assert_eq!(baseline_stability_days(95), 7.0);
        assert!(baseline_difficulty(20, 4) > baseline_difficulty(80, 0));
    }
}
