pub mod github;
pub mod gitlab;
pub mod notion;
pub mod plane;
pub mod traits;
pub mod webhook;

pub use github::GitHubClient;
pub use gitlab::GitLabClient;
pub use plane::{
    IssueCandidateData, PaginatedResponse, PlaneClient, PlaneProject, PlaneState, PlaneUser,
    PlaneWorkItem, PlaneWorklog, PlaneWorkspace, WorklogSummary,
};
pub use traits::{
    CreateIssueRequest, CreatedIssue, IssueDetails, IssueManagement, IssueState,
    IssueSyncReport, ProjectManagementSystem, SyncReport, TimeEntry, UpdateIssueRequest,
    WorkItemDetails,
};
pub use webhook::{
    verify_webhook_signature, PlaneEventType, PlaneWebhookPayload, WebhookResult, WebhookWorkItem,
    process_webhook,
};
pub use notion::{
    NotionClient, NotionDatabase, NotionPage, NotionBlock, NotionPropertyValue,
    NotionPropertyUpdate, NotionIssueCandidateData, NotionPaginatedResponse,
    PropertyMapping, PropertyMappingConfig,
};
