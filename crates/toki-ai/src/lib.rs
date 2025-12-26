pub mod auto_linker;
pub mod embedding;
pub mod gravity;
pub mod insights;
pub mod issue_matcher;
pub mod issue_sync;
pub mod notion_issue_sync;
pub mod notion_mapper;
pub mod rules;
pub mod standup;
pub mod time_analyzer;
pub mod work_summary;

pub use auto_linker::{AutoLinker, LinkReason, LinkSuggestion};
pub use embedding::EmbeddingService;
pub use gravity::{GravityCalculator, RelevanceStatus};
pub use insights::InsightsGenerator;
pub use issue_matcher::{
    ActivitySignals, CandidateIssue, IssueMatch, IssueMatcher, MatchReason, SmartIssueMatcher,
};
pub use issue_sync::{IssueSyncService, SyncStats};
pub use notion_issue_sync::{NotionIssueSyncService, SyncOptions, SyncOutcome, SyncResult, SyncTarget};
pub use notion_mapper::{IssueMappingConfig, NotionIssueMapper};
pub use rules::RuleEngine;
pub use time_analyzer::{
    ActivitySegment, DailySummaryReport, SuggestedIssue, SuggestedTimeBlock, TimeAnalyzer,
    WorkPattern,
};
pub use standup::{ProjectStandupItem, StandupFormat, StandupGenerator, StandupReport};
pub use work_summary::{ProjectWorkSummary, SummaryPeriod, WorkSummary, WorkSummaryGenerator};
