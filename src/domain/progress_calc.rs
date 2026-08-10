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
}
