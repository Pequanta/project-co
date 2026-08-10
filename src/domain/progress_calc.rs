/// Deterministic, pluggable progress calculation.
///
/// Default: `progress = completed / total_active * 100` where
/// `total_active = completed + in_progress + planned` (cancelled excluded).
/// Implement `ProgressCalculator` to switch strategy (e.g. weighted plans).
pub trait ProgressCalculator: Send + Sync {
    fn calculate(&self, counts: PlanCounts) -> u8;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct PlanCounts {
    pub completed: i64,
    pub in_progress: i64,
    pub planned: i64,
    pub cancelled: i64,
}

impl PlanCounts {
    pub fn total_active(&self) -> i64 {
        self.completed + self.in_progress + self.planned
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SimpleProgressCalculator;

impl ProgressCalculator for SimpleProgressCalculator {
    fn calculate(&self, counts: PlanCounts) -> u8 {
        let total = counts.total_active();
        if total == 0 {
            return 0;
        }
        let pct = (counts.completed as f64 / total as f64) * 100.0;
        pct.round().clamp(0.0, 100.0) as u8
    }
}

/// Visual bar, e.g. 60% width 10 -> `██████░░░░`.
pub fn progress_bar(percent: u8, width: usize) -> String {
    let filled = (percent as usize * width / 100).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// A single member's progress: plans they completed over the total active
/// plans. Same computation in both study and collaboration modes.
pub fn member_percent(my_completed: i64, total_active: i64) -> u8 {
    if total_active <= 0 {
        return 0;
    }
    let pct = (my_completed.max(0) as f64 / total_active as f64) * 100.0;
    pct.round().clamp(0.0, 100.0) as u8
}

/// Overall progress for a study session: the average of each member's
/// percentage. Equivalent to `sum(completions) / (members * total_active)`
/// since every member shares the same denominator.
pub fn study_overall_percent(member_completions: &[i64], total_active: i64) -> u8 {
    let members = member_completions.len() as i64;
    if members == 0 || total_active <= 0 {
        return 0;
    }
    let done: i64 = member_completions
        .iter()
        .map(|c| (*c).clamp(0, total_active))
        .sum();
    let pct = (done as f64 / (members * total_active) as f64) * 100.0;
    pct.round().clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(c: i64, ip: i64, p: i64, x: i64) -> PlanCounts {
        PlanCounts {
            completed: c,
            in_progress: ip,
            planned: p,
            cancelled: x,
        }
    }

    #[test]
    fn zero_plans_is_zero_percent() {
        assert_eq!(SimpleProgressCalculator.calculate(counts(0, 0, 0, 0)), 0);
    }

    #[test]
    fn six_of_ten_is_sixty() {
        assert_eq!(SimpleProgressCalculator.calculate(counts(6, 2, 2, 0)), 60);
    }

    #[test]
    fn cancelled_plans_do_not_count() {
        // 6 done, 4 cancelled -> only 6 active plans, all completed -> 100%.
        assert_eq!(SimpleProgressCalculator.calculate(counts(6, 0, 0, 4)), 100);
    }

    #[test]
    fn rounding() {
        assert_eq!(SimpleProgressCalculator.calculate(counts(1, 0, 2, 0)), 33);
        assert_eq!(SimpleProgressCalculator.calculate(counts(2, 0, 1, 0)), 67);
    }

    #[test]
    fn bar_matches_percent() {
        assert_eq!(progress_bar(60, 10), "██████░░░░");
        assert_eq!(progress_bar(0, 10), "░░░░░░░░░░");
        assert_eq!(progress_bar(100, 10), "██████████");
    }

    #[test]
    fn member_percent_basic() {
        assert_eq!(member_percent(0, 0), 0);
        assert_eq!(member_percent(5, 0), 0);
        assert_eq!(member_percent(3, 10), 30);
        assert_eq!(member_percent(1, 3), 33);
        assert_eq!(member_percent(2, 3), 67);
        assert_eq!(member_percent(10, 10), 100);
    }

    #[test]
    fn study_overall_is_average_of_members() {
        // Two members: 3/10 and 5/10 -> (30 + 50) / 2 = 40.
        assert_eq!(study_overall_percent(&[3, 5], 10), 40);
        // Everyone done -> 100.
        assert_eq!(study_overall_percent(&[10, 10, 10], 10), 100);
        // No plans or no members -> 0.
        assert_eq!(study_overall_percent(&[0, 0], 0), 0);
        assert_eq!(study_overall_percent(&[], 10), 0);
    }

    #[test]
    fn collaboration_shares_sum_to_overall() {
        // 6 of 10 active plans done, split 4 + 2 between two members.
        let total_active = 10;
        let a = member_percent(4, total_active);
        let b = member_percent(2, total_active);
        let overall = SimpleProgressCalculator.calculate(counts(6, 2, 2, 0));
        assert_eq!(a + b, overall);
    }
}
