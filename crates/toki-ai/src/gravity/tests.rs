use super::*;

// ============================================================================
// RelevanceStatus enum tests
// ============================================================================

#[test]
fn test_relevance_status_clone() {
    let status = RelevanceStatus::Focus;
    let cloned = status.clone();
    assert_eq!(cloned, RelevanceStatus::Focus);
}

#[test]
fn test_relevance_status_copy() {
    let status = RelevanceStatus::Drift;
    let copied = status; // Copy, not move
    assert_eq!(copied, RelevanceStatus::Drift);
    assert_eq!(status, RelevanceStatus::Drift); // Original still usable
}

#[test]
fn test_relevance_status_debug() {
    let focus = RelevanceStatus::Focus;
    let drift = RelevanceStatus::Drift;
    let break_status = RelevanceStatus::Break;

    assert!(format!("{:?}", focus).contains("Focus"));
    assert!(format!("{:?}", drift).contains("Drift"));
    assert!(format!("{:?}", break_status).contains("Break"));
}

#[test]
fn test_relevance_status_partial_eq() {
    assert_eq!(RelevanceStatus::Focus, RelevanceStatus::Focus);
    assert_eq!(RelevanceStatus::Drift, RelevanceStatus::Drift);
    assert_eq!(RelevanceStatus::Break, RelevanceStatus::Break);

    assert_ne!(RelevanceStatus::Focus, RelevanceStatus::Drift);
    assert_ne!(RelevanceStatus::Focus, RelevanceStatus::Break);
    assert_ne!(RelevanceStatus::Drift, RelevanceStatus::Break);
}

// ============================================================================
// RelevanceStatus::from_score tests
// ============================================================================

#[test]
fn test_from_score_focus_high() {
    let status = RelevanceStatus::from_score(1.0);
    assert_eq!(status, RelevanceStatus::Focus);
}

#[test]
fn test_from_score_focus_at_threshold() {
    let status = RelevanceStatus::from_score(0.6);
    assert_eq!(status, RelevanceStatus::Focus);
}

#[test]
fn test_from_score_focus_above_threshold() {
    let status = RelevanceStatus::from_score(0.75);
    assert_eq!(status, RelevanceStatus::Focus);
}

#[test]
fn test_from_score_drift_just_below_focus() {
    let status = RelevanceStatus::from_score(0.59);
    assert_eq!(status, RelevanceStatus::Drift);
}

#[test]
fn test_from_score_drift_at_threshold() {
    let status = RelevanceStatus::from_score(0.3);
    assert_eq!(status, RelevanceStatus::Drift);
}

#[test]
fn test_from_score_drift_middle() {
    let status = RelevanceStatus::from_score(0.45);
    assert_eq!(status, RelevanceStatus::Drift);
}

#[test]
fn test_from_score_break_just_below_drift() {
    let status = RelevanceStatus::from_score(0.29);
    assert_eq!(status, RelevanceStatus::Break);
}

#[test]
fn test_from_score_break_low() {
    let status = RelevanceStatus::from_score(0.1);
    assert_eq!(status, RelevanceStatus::Break);
}

#[test]
fn test_from_score_break_zero() {
    let status = RelevanceStatus::from_score(0.0);
    assert_eq!(status, RelevanceStatus::Break);
}

#[test]
fn test_from_score_break_negative() {
    // Negative scores should be Break
    let status = RelevanceStatus::from_score(-0.5);
    assert_eq!(status, RelevanceStatus::Break);
}

#[test]
fn test_from_score_focus_above_one() {
    // Scores > 1.0 should still be Focus
    let status = RelevanceStatus::from_score(1.5);
    assert_eq!(status, RelevanceStatus::Focus);
}

// ============================================================================
// Boundary tests for from_score
// ============================================================================

#[test]
fn test_from_score_boundary_focus_drift() {
    // Just at 0.6 should be Focus
    assert_eq!(RelevanceStatus::from_score(0.6), RelevanceStatus::Focus);
    // Just below 0.6 should be Drift
    assert_eq!(RelevanceStatus::from_score(0.599), RelevanceStatus::Drift);
}

#[test]
fn test_from_score_boundary_drift_break() {
    // Just at 0.3 should be Drift
    assert_eq!(RelevanceStatus::from_score(0.3), RelevanceStatus::Drift);
    // Just below 0.3 should be Break
    assert_eq!(RelevanceStatus::from_score(0.299), RelevanceStatus::Break);
}

#[test]
fn test_from_score_epsilon_precision() {
    // Test with very small differences
    let epsilon = 0.0001;

    // Just above 0.6
    assert_eq!(RelevanceStatus::from_score(0.6 + epsilon), RelevanceStatus::Focus);
    // Just below 0.6
    assert_eq!(RelevanceStatus::from_score(0.6 - epsilon), RelevanceStatus::Drift);

    // Just above 0.3
    assert_eq!(RelevanceStatus::from_score(0.3 + epsilon), RelevanceStatus::Drift);
    // Just below 0.3
    assert_eq!(RelevanceStatus::from_score(0.3 - epsilon), RelevanceStatus::Break);
}

// ============================================================================
// Edge case tests
// ============================================================================

#[test]
fn test_from_score_nan_behavior() {
    // NaN comparisons are always false, so score >= 0.6 is false, score >= 0.3 is false
    // This means NaN should result in Break
    let status = RelevanceStatus::from_score(f32::NAN);
    assert_eq!(status, RelevanceStatus::Break);
}

#[test]
fn test_from_score_infinity() {
    // Positive infinity should be Focus (greater than 0.6)
    let status = RelevanceStatus::from_score(f32::INFINITY);
    assert_eq!(status, RelevanceStatus::Focus);
}

#[test]
fn test_from_score_negative_infinity() {
    // Negative infinity should be Break (less than 0.3)
    let status = RelevanceStatus::from_score(f32::NEG_INFINITY);
    assert_eq!(status, RelevanceStatus::Break);
}

// ============================================================================
// Integration tests for RelevanceStatus
// ============================================================================

#[test]
fn test_score_ranges_comprehensive() {
    // Test a range of scores to ensure correct categorization
    let focus_scores = [0.6, 0.7, 0.8, 0.9, 1.0];
    let drift_scores = [0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.59];
    let break_scores = [0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.29];

    for score in focus_scores {
        assert_eq!(
            RelevanceStatus::from_score(score),
            RelevanceStatus::Focus,
            "Score {} should be Focus",
            score
        );
    }

    for score in drift_scores {
        assert_eq!(
            RelevanceStatus::from_score(score),
            RelevanceStatus::Drift,
            "Score {} should be Drift",
            score
        );
    }

    for score in break_scores {
        assert_eq!(
            RelevanceStatus::from_score(score),
            RelevanceStatus::Break,
            "Score {} should be Break",
            score
        );
    }
}

#[test]
fn test_relevance_status_all_variants() {
    // Ensure we can create all variants
    let variants = [
        RelevanceStatus::Focus,
        RelevanceStatus::Drift,
        RelevanceStatus::Break,
    ];

    assert_eq!(variants.len(), 3);

    // Each variant is distinct
    for (i, v1) in variants.iter().enumerate() {
        for (j, v2) in variants.iter().enumerate() {
            if i == j {
                assert_eq!(v1, v2);
            } else {
                assert_ne!(v1, v2);
            }
        }
    }
}
