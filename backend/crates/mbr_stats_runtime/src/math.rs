//! Shared math helpers used by queries.rs and ft_dashboard.rs.
//! All functions are pub(crate) — internal to mbr_stats_runtime.

pub(crate) fn ratio_to_float_f64(numerator: f64, denominator: f64) -> Option<f64> {
    (denominator > 0.0).then_some(numerator / denominator)
}

pub(crate) fn roi_from_totals(total_payout_cents: i64, total_buyin_cents: i64) -> Option<f64> {
    if total_buyin_cents == 0 {
        None
    } else {
        Some(((total_payout_cents - total_buyin_cents) as f64 / total_buyin_cents as f64) * 100.0)
    }
}
