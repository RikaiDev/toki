pub mod context;
pub mod git;
pub mod ide;
pub mod parser;

pub use context::{DetectionSource, WorkContextDetector, WorkItemRef};
pub use git::GitDetector;
pub use parser::{IssueId, IssueIdParser};
