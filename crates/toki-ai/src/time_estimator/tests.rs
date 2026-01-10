use super::*;

// ==================== TimeEstimate::format_duration tests ====================

#[test]
fn test_format_duration_zero() {
    assert_eq!(TimeEstimate::format_duration(0), "< 1m");
}

#[test]
fn test_format_duration_seconds_only() {
    assert_eq!(TimeEstimate::format_duration(30), "30s");
    assert_eq!(TimeEstimate::format_duration(59), "59s");
}

#[test]
fn test_format_duration_minutes_only() {
    assert_eq!(TimeEstimate::format_duration(60), "1m");
    assert_eq!(TimeEstimate::format_duration(120), "2m");
    assert_eq!(TimeEstimate::format_duration(45 * 60), "45m");
}

#[test]
fn test_format_duration_hours_only() {
    assert_eq!(TimeEstimate::format_duration(3600), "1h");
    assert_eq!(TimeEstimate::format_duration(2 * 3600), "2h");
    assert_eq!(TimeEstimate::format_duration(24 * 3600), "24h");
}

#[test]
fn test_format_duration_hours_and_minutes() {
    assert_eq!(TimeEstimate::format_duration(3600 + 30 * 60), "1h 30m");
    assert_eq!(TimeEstimate::format_duration(2 * 3600 + 15 * 60), "2h 15m");
    assert_eq!(TimeEstimate::format_duration(10 * 3600 + 5 * 60), "10h 5m");
}

#[test]
fn test_format_duration_ignores_leftover_seconds() {
    // Hours and minutes present, seconds ignored
    assert_eq!(TimeEstimate::format_duration(3600 + 30 * 60 + 45), "1h 30m");
    // Minutes present, seconds ignored
    assert_eq!(TimeEstimate::format_duration(5 * 60 + 30), "5m");
}

// ==================== TimeEstimate methods tests ====================

#[test]
fn test_time_estimate_formatted() {
    let estimate = TimeEstimate {
        estimated_seconds: 7200, // 2 hours
        low_seconds: 3600,
        high_seconds: 10800,
        confidence: 0.8,
        similar_issues: vec![],
        method: EstimationMethod::ComplexityBased,
        breakdown: None,
    };
    assert_eq!(estimate.formatted(), "2h");
}

#[test]
fn test_time_estimate_formatted_range() {
    let estimate = TimeEstimate {
        estimated_seconds: 7200,
        low_seconds: 3600,       // 1 hour
        high_seconds: 14400,     // 4 hours
        confidence: 0.7,
        similar_issues: vec![],
        method: EstimationMethod::ComplexityBased,
        breakdown: None,
    };
    assert_eq!(estimate.formatted_range(), "1h - 4h");
}

// ==================== TimeBreakdown tests ====================

#[test]
fn test_time_breakdown_from_total() {
    let breakdown = TimeBreakdown::from_total(10000);
    assert_eq!(breakdown.implementation_seconds, 6000); // 60%
    assert_eq!(breakdown.testing_seconds, 3000);        // 30%
    assert_eq!(breakdown.documentation_seconds, 1000);  // 10%
}

#[test]
fn test_time_breakdown_from_total_zero() {
    let breakdown = TimeBreakdown::from_total(0);
    assert_eq!(breakdown.implementation_seconds, 0);
    assert_eq!(breakdown.testing_seconds, 0);
    assert_eq!(breakdown.documentation_seconds, 0);
}

#[test]
fn test_time_breakdown_from_total_small() {
    // Test with small values where integer division matters
    let breakdown = TimeBreakdown::from_total(100);
    assert_eq!(breakdown.implementation_seconds, 60);
    assert_eq!(breakdown.testing_seconds, 30);
    assert_eq!(breakdown.documentation_seconds, 10);
}

// ==================== EstimationMethod Display tests ====================

#[test]
fn test_estimation_method_display_similar_issues() {
    assert_eq!(format!("{}", EstimationMethod::SimilarIssues), "Similar issues");
}

#[test]
fn test_estimation_method_display_complexity_based() {
    assert_eq!(format!("{}", EstimationMethod::ComplexityBased), "Complexity-based");
}

#[test]
fn test_estimation_method_display_combined() {
    assert_eq!(format!("{}", EstimationMethod::Combined), "Combined analysis");
}

#[test]
fn test_estimation_method_display_ai_rag() {
    assert_eq!(
        format!("{}", EstimationMethod::AiRag("gpt-4".to_string())),
        "AI Estimation (gpt-4)"
    );
}

#[test]
fn test_estimation_method_equality() {
    assert_eq!(EstimationMethod::SimilarIssues, EstimationMethod::SimilarIssues);
    assert_eq!(EstimationMethod::ComplexityBased, EstimationMethod::ComplexityBased);
    assert_eq!(EstimationMethod::Combined, EstimationMethod::Combined);
    assert_eq!(
        EstimationMethod::AiRag("model".to_string()),
        EstimationMethod::AiRag("model".to_string())
    );
    assert_ne!(EstimationMethod::SimilarIssues, EstimationMethod::ComplexityBased);
}

// ==================== cosine_similarity tests ====================

#[test]
fn test_cosine_similarity_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - 1.0).abs() < 0.0001);
}

#[test]
fn test_cosine_similarity_orthogonal_vectors() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!(sim.abs() < 0.0001);
}

#[test]
fn test_cosine_similarity_opposite_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![-1.0, -2.0, -3.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim + 1.0).abs() < 0.0001);
}

#[test]
fn test_cosine_similarity_different_lengths() {
    let a = vec![1.0, 2.0];
    let b = vec![1.0, 2.0, 3.0];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn test_cosine_similarity_empty_vectors() {
    let a: Vec<f32> = vec![];
    let b: Vec<f32> = vec![];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn test_cosine_similarity_zero_vector() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 2.0, 3.0];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn test_cosine_similarity_partial() {
    // Vectors at 45 degrees should have similarity ~0.707
    let a = vec![1.0, 0.0];
    let b = vec![1.0, 1.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - 0.7071).abs() < 0.001);
}

// ==================== estimate_from_complexity tests ====================

#[test]
fn test_estimate_from_complexity_trivial() {
    let estimate = TimeEstimator::estimate_from_complexity(Complexity::Trivial);
    assert_eq!(estimate.estimated_seconds, 5 * 60); // 5 minutes
    assert_eq!(estimate.low_seconds, 5 * 60 * 50 / 100); // 2.5 minutes
    assert_eq!(estimate.high_seconds, 5 * 60 * 200 / 100); // 10 minutes
    assert_eq!(estimate.confidence, 0.5);
    assert_eq!(estimate.method, EstimationMethod::ComplexityBased);
    assert!(estimate.similar_issues.is_empty());
}

#[test]
fn test_estimate_from_complexity_simple() {
    let estimate = TimeEstimator::estimate_from_complexity(Complexity::Simple);
    assert_eq!(estimate.estimated_seconds, 30 * 60); // 30 minutes
    assert_eq!(estimate.low_seconds, 30 * 60 * 50 / 100); // 15 minutes
    assert_eq!(estimate.high_seconds, 30 * 60 * 200 / 100); // 60 minutes
}

#[test]
fn test_estimate_from_complexity_moderate() {
    let estimate = TimeEstimator::estimate_from_complexity(Complexity::Moderate);
    assert_eq!(estimate.estimated_seconds, 2 * 3600); // 2 hours
    assert_eq!(estimate.low_seconds, 2 * 3600 * 50 / 100); // 1 hour
    assert_eq!(estimate.high_seconds, 2 * 3600 * 200 / 100); // 4 hours
}

#[test]
fn test_estimate_from_complexity_complex() {
    let estimate = TimeEstimator::estimate_from_complexity(Complexity::Complex);
    assert_eq!(estimate.estimated_seconds, 6 * 3600); // 6 hours
    assert_eq!(estimate.low_seconds, 6 * 3600 * 50 / 100); // 3 hours
    assert_eq!(estimate.high_seconds, 6 * 3600 * 200 / 100); // 12 hours
}

#[test]
fn test_estimate_from_complexity_epic() {
    let estimate = TimeEstimator::estimate_from_complexity(Complexity::Epic);
    assert_eq!(estimate.estimated_seconds, 20 * 3600); // 20 hours
    assert_eq!(estimate.low_seconds, 20 * 3600 * 50 / 100); // 10 hours
    assert_eq!(estimate.high_seconds, 20 * 3600 * 250 / 100); // 50 hours
}

#[test]
fn test_estimate_from_complexity_has_breakdown() {
    let estimate = TimeEstimator::estimate_from_complexity(Complexity::Moderate);
    assert!(estimate.breakdown.is_some());
    let breakdown = estimate.breakdown.unwrap();
    assert_eq!(breakdown.implementation_seconds, 2 * 3600 * 60 / 100);
    assert_eq!(breakdown.testing_seconds, 2 * 3600 * 30 / 100);
    assert_eq!(breakdown.documentation_seconds, 2 * 3600 * 10 / 100);
}

// ==================== estimate_from_similar tests ====================

fn create_similar_issue(id: &str, seconds: u32, similarity: f32) -> SimilarIssue {
    SimilarIssue {
        issue_id: id.to_string(),
        title: format!("Issue {id}"),
        actual_seconds: seconds,
        complexity: None,
        similarity,
    }
}

#[test]
fn test_estimate_from_similar_single_issue() {
    let similar = vec![create_similar_issue("1", 3600, 0.9)];
    let estimate = TimeEstimator::estimate_from_similar(&similar, None);

    assert_eq!(estimate.estimated_seconds, 3600);
    assert_eq!(estimate.method, EstimationMethod::SimilarIssues);
    assert_eq!(estimate.similar_issues.len(), 1);
}

#[test]
fn test_estimate_from_similar_weighted_average() {
    // Two issues: one at 1 hour with 0.8 similarity, one at 2 hours with 0.2 similarity
    // Weighted average: (3600 * 0.8 + 7200 * 0.2) / (0.8 + 0.2) = 4320
    let similar = vec![
        create_similar_issue("1", 3600, 0.8),
        create_similar_issue("2", 7200, 0.2),
    ];
    let estimate = TimeEstimator::estimate_from_similar(&similar, None);

    assert_eq!(estimate.estimated_seconds, 4320);
}

#[test]
fn test_estimate_from_similar_with_complexity_uses_combined() {
    let similar = vec![create_similar_issue("1", 3600, 0.9)];
    let estimate = TimeEstimator::estimate_from_similar(&similar, Some(Complexity::Moderate));

    // Should blend: 70% similar (3600) + 30% complexity (7200) = 2520 + 2160 = 4680
    let expected = 3600 * 70 / 100 + (2 * 3600) * 30 / 100;
    assert_eq!(estimate.estimated_seconds, expected);
    assert_eq!(estimate.method, EstimationMethod::Combined);
}

#[test]
fn test_estimate_from_similar_confidence_calculation() {
    // 5 issues with high similarity should give high confidence
    let similar: Vec<SimilarIssue> = (1..=5)
        .map(|i| create_similar_issue(&i.to_string(), 3600, 0.9))
        .collect();
    let estimate = TimeEstimator::estimate_from_similar(&similar, None);

    // avg_similarity = 0.9, count_factor = 5/5 = 1.0, confidence = 0.9 * 1.0 = 0.9
    assert!((estimate.confidence - 0.9).abs() < 0.01);
}

#[test]
fn test_estimate_from_similar_low_count_reduces_confidence() {
    // Only 2 issues should reduce confidence
    let similar = vec![
        create_similar_issue("1", 3600, 0.9),
        create_similar_issue("2", 3600, 0.9),
    ];
    let estimate = TimeEstimator::estimate_from_similar(&similar, None);

    // avg_similarity = 0.9, count_factor = 2/5 = 0.4, confidence = 0.9 * 0.4 = 0.36
    assert!((estimate.confidence - 0.36).abs() < 0.01);
}

#[test]
fn test_estimate_from_similar_has_breakdown() {
    let similar = vec![create_similar_issue("1", 10000, 0.9)];
    let estimate = TimeEstimator::estimate_from_similar(&similar, None);

    assert!(estimate.breakdown.is_some());
    let breakdown = estimate.breakdown.unwrap();
    assert_eq!(breakdown.implementation_seconds, 10000 * 60 / 100);
}

#[test]
fn test_estimate_from_similar_equal_weights() {
    // Three issues with equal similarity should give simple average
    let similar = vec![
        create_similar_issue("1", 1000, 0.8),
        create_similar_issue("2", 2000, 0.8),
        create_similar_issue("3", 3000, 0.8),
    ];
    let estimate = TimeEstimator::estimate_from_similar(&similar, None);

    // Weighted average with equal weights = simple average = 2000
    assert_eq!(estimate.estimated_seconds, 2000);
}

// ==================== SimilarIssue struct tests ====================

#[test]
fn test_similar_issue_creation() {
    let issue = SimilarIssue {
        issue_id: "TEST-123".to_string(),
        title: "Fix login bug".to_string(),
        actual_seconds: 7200,
        complexity: Some(Complexity::Simple),
        similarity: 0.85,
    };

    assert_eq!(issue.issue_id, "TEST-123");
    assert_eq!(issue.title, "Fix login bug");
    assert_eq!(issue.actual_seconds, 7200);
    assert_eq!(issue.complexity, Some(Complexity::Simple));
    assert!((issue.similarity - 0.85).abs() < 0.001);
}

#[test]
fn test_similar_issue_clone() {
    let issue = create_similar_issue("1", 3600, 0.9);
    let cloned = issue.clone();
    assert_eq!(issue.issue_id, cloned.issue_id);
    assert_eq!(issue.actual_seconds, cloned.actual_seconds);
}
