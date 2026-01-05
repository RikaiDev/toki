pub mod db;
pub mod encryption;
pub mod migrations;
pub mod models;

pub use db::{Database, IssueTimeStats};
pub use encryption::{default_key_path, generate_key, load_key_from_file, save_key_to_file};
pub use models::{
    Activity, ActivityContext, ActivitySpan, ActivitySpanContext, AiConfig, AiProvider, Category,
    ClassificationRule, ClaudeSession, Complexity, DailySummary, IntegrationConfig, IssueCandidate,
    PatternType, Project, ProjectSummary, Session, Settings, TimeBlock, TimeBlockSource, WorkItem,
};
