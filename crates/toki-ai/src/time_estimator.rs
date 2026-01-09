//! Time Estimation Module
//!
//! AI-powered time estimation based on:
//! - Historical data from similar issues
//! - Issue complexity
//! - Embedding similarity

use std::sync::Arc;

use crate::ai_service::AiService;
use anyhow::Result;
use toki_storage::{Complexity, Database, IssueCandidate, IssueTimeStats};

/// Time estimate result
#[derive(Debug, Clone)]
pub struct TimeEstimate {
    /// Estimated time in seconds
    pub estimated_seconds: u32,
    /// Lower bound (optimistic)
    pub low_seconds: u32,
    /// Upper bound (pessimistic)
    pub high_seconds: u32,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Similar issues used for estimation
    pub similar_issues: Vec<SimilarIssue>,
    /// Estimation method used
    pub method: EstimationMethod,
    /// Suggested breakdown
    pub breakdown: Option<TimeBreakdown>,
}

impl TimeEstimate {
    /// Format duration in human-readable form
    #[must_use] pub fn format_duration(seconds: u32) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;

        if hours > 0 {
            if minutes > 0 {
                format!("{hours}h {minutes}m")
            } else {
                format!("{hours}h")
            }
        } else if minutes > 0 {
            format!("{minutes}m")
        } else if seconds > 0 {
            format!("{seconds}s")
        } else {
            "< 1m".to_string()
        }
    }

    /// Get formatted estimate string
    #[must_use]
    pub fn formatted(&self) -> String {
        Self::format_duration(self.estimated_seconds)
    }

    /// Get formatted range string
    #[must_use]
    pub fn formatted_range(&self) -> String {
        format!(
            "{} - {}",
            Self::format_duration(self.low_seconds),
            Self::format_duration(self.high_seconds)
        )
    }
}

/// Similar issue with time data
#[derive(Debug, Clone)]
pub struct SimilarIssue {
    pub issue_id: String,
    pub title: String,
    pub actual_seconds: u32,
    pub complexity: Option<Complexity>,
    pub similarity: f32,
}

/// Time breakdown by task type
#[derive(Debug, Clone)]
pub struct TimeBreakdown {
    pub implementation_seconds: u32,
    pub testing_seconds: u32,
    pub documentation_seconds: u32,
}

impl TimeBreakdown {
    /// Create breakdown from total time based on typical ratios
    #[must_use]
    pub fn from_total(total_seconds: u32) -> Self {
        // Typical ratio: 60% implementation, 30% testing, 10% documentation
        // Use integer arithmetic to avoid float precision warnings
        Self {
            implementation_seconds: total_seconds * 60 / 100,
            testing_seconds: total_seconds * 30 / 100,
            documentation_seconds: total_seconds * 10 / 100,
        }
    }
}

/// Estimation method used
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstimationMethod {
    /// Based on similar issues with historical data
    SimilarIssues,
    /// Based on complexity alone (no historical data)
    ComplexityBased,
    /// Combination of both
    Combined,
    /// AI RAG estimation (with model name)
    AiRag(String),
}

impl std::fmt::Display for EstimationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimilarIssues => write!(f, "Similar issues"),
            Self::ComplexityBased => write!(f, "Complexity-based"),
            Self::Combined => write!(f, "Combined analysis"),
            Self::AiRag(model) => write!(f, "AI Estimation ({model})"),
        }
    }
}

/// Time estimator using historical data, embeddings, and generative AI
pub struct TimeEstimator {
    db: Arc<Database>,
    ai_service: Option<AiService>,
}

impl TimeEstimator {
    /// Create a new time estimator
    #[must_use]
    pub fn new(db: Arc<Database>, ai_service: Option<AiService>) -> Self {
        Self { db, ai_service }
    }

    /// Estimate time for an issue
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail or AI service fails
    pub async fn estimate(&self, issue: &IssueCandidate) -> Result<TimeEstimate> {
        // Get historical time stats
        let time_stats = self.db.get_issue_time_stats()?;

        // Find similar issues using embeddings
        // Note: find_similar_issues is synchronous as it uses local embeddings
        let similar_issues = self.find_similar_issues(issue, &time_stats)?;

        // Try AI estimation first if available
        if let Some(ai) = &self.ai_service {
            // Needed for prompt context
            let similar_stats: Vec<IssueTimeStats> = similar_issues
                .iter()
                .filter_map(|s| {
                    time_stats
                        .iter()
                        .find(|t| t.issue_id == s.issue_id)
                        .cloned()
                })
                .collect();

            if let Ok(seconds) = ai.estimate_time_rag(issue, &similar_stats).await {
                return Ok(TimeEstimate {
                    estimated_seconds: seconds,
                    low_seconds: seconds * 80 / 100,
                    high_seconds: seconds * 120 / 100,
                    confidence: 0.8, // High confidence for AI
                    similar_issues,
                    method: EstimationMethod::AiRag(ai.model_name().to_string()),
                    breakdown: Some(TimeBreakdown::from_total(seconds)),
                });
            }
            // If AI fails, fall back to heuristic
            log::warn!("AI estimation failed, falling back to heuristics");
        }

        if !similar_issues.is_empty() {
            // Use similar issues for estimation
            Ok(Self::estimate_from_similar(&similar_issues, issue.complexity))
        } else if let Some(complexity) = issue.complexity {
            // Fall back to complexity-based estimation
            Ok(Self::estimate_from_complexity(complexity))
        } else {
            // Default estimation based on "moderate" complexity
            Ok(Self::estimate_from_complexity(Complexity::Moderate))
        }
    }

    /// Find similar issues with historical time data
    fn find_similar_issues(
        &self,
        issue: &IssueCandidate,
        time_stats: &[IssueTimeStats],
    ) -> Result<Vec<SimilarIssue>> {
        let mut similar = Vec::new();

        // If issue has embedding, use it to find similar
        if let Some(ref embedding) = issue.embedding {
            for stats in time_stats {
                // Look up the issue candidate to get its embedding
                if let Some(candidate) =
                    self.db.get_issue_candidate(&stats.issue_id, &stats.issue_system)?
                {
                    if let Some(ref candidate_embedding) = candidate.embedding {
                        let similarity = cosine_similarity(embedding, candidate_embedding);

                        // Only include issues with decent similarity
                        if similarity > 0.5 {
                            similar.push(SimilarIssue {
                                issue_id: stats.issue_id.clone(),
                                title: candidate.title.clone(),
                                actual_seconds: stats.total_seconds,
                                complexity: candidate.complexity,
                                similarity,
                            });
                        }
                    }
                }
            }

            // Sort by similarity (highest first)
            similar.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

            // Keep top 5
            similar.truncate(5);
        }

        Ok(similar)
    }

    /// Estimate from similar issues
    fn estimate_from_similar(
        similar: &[SimilarIssue],
        complexity: Option<Complexity>,
    ) -> TimeEstimate {
        // Weighted average based on similarity (use f64 for precision)
        let total_weight: f64 = similar.iter().map(|s| f64::from(s.similarity)).sum();
        let weighted_sum: f64 = similar
            .iter()
            .map(|s| f64::from(s.actual_seconds) * f64::from(s.similarity))
            .sum();

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let estimated = (weighted_sum / total_weight) as u32;

        // Calculate variance for confidence interval
        let times: Vec<f64> = similar.iter().map(|s| f64::from(s.actual_seconds)).collect();
        let mean = f64::from(estimated);
        // similar.len() is at most 5 (truncated in find_similar_issues), safe to cast to u8
        let len = f64::from(u8::try_from(similar.len()).unwrap_or(5));
        let variance: f64 = times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / len;
        let std_dev = variance.sqrt();

        // 80% confidence interval (roughly 1.28 standard deviations)
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let low = (mean - 1.28 * std_dev).max(0.0) as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let high = (mean + 1.28 * std_dev) as u32;

        // Confidence based on number of similar issues and their similarity
        // similar.len() is at most 5, safe to cast to u8
        let similar_count = u8::try_from(similar.len()).unwrap_or(5);
        let avg_similarity: f32 = similar.iter().map(|s| s.similarity).sum::<f32>() / f32::from(similar_count);
        let count_factor = (f32::from(similar_count) / 5.0).min(1.0);
        let confidence = avg_similarity * count_factor;

        // If we have complexity, adjust the estimate
        let (final_estimate, method) = if let Some(c) = complexity {
            let complexity_estimate = Self::estimate_from_complexity(c);
            // Blend: 70% similar issues, 30% complexity-based
            let blended = estimated * 70 / 100 + complexity_estimate.estimated_seconds * 30 / 100;
            (blended, EstimationMethod::Combined)
        } else {
            (estimated, EstimationMethod::SimilarIssues)
        };

        TimeEstimate {
            estimated_seconds: final_estimate,
            low_seconds: low.min(final_estimate),
            high_seconds: high.max(final_estimate),
            confidence,
            similar_issues: similar.to_vec(),
            method,
            breakdown: Some(TimeBreakdown::from_total(final_estimate)),
        }
    }

    /// Estimate from complexity alone
    fn estimate_from_complexity(complexity: Complexity) -> TimeEstimate {
        // Base estimates in seconds per complexity level
        // Based on typical AI-assisted development times
        // Factors are represented as (low_percent, high_percent) to avoid float casting
        let (base_seconds, low_percent, high_percent) = match complexity {
            Complexity::Trivial => (5 * 60, 50, 200),      // 5 min (2.5-10 min)
            Complexity::Simple => (30 * 60, 50, 200),     // 30 min (15-60 min)
            Complexity::Moderate => (2 * 3600, 50, 200),  // 2 hours (1-4 hours)
            Complexity::Complex => (6 * 3600, 50, 200),   // 6 hours (3-12 hours)
            Complexity::Epic => (20 * 3600, 50, 250),     // 20 hours (10-50 hours)
        };

        let low = base_seconds * low_percent / 100;
        let high = base_seconds * high_percent / 100;

        TimeEstimate {
            estimated_seconds: base_seconds,
            low_seconds: low,
            high_seconds: high,
            confidence: 0.5, // Medium confidence for complexity-only estimates
            similar_issues: Vec::new(),
            method: EstimationMethod::ComplexityBased,
            breakdown: Some(TimeBreakdown::from_total(base_seconds)),
        }
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}
