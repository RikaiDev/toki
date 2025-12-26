//! Toki MCP Server
//!
//! Model Context Protocol server for AI agents to interact with Toki.
//! Provides tools for Notion integration and issue sync operations.

mod handlers;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

use handlers::TokiService;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to stderr (stdout is used for MCP protocol)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    log::info!("Starting Toki MCP server...");

    // Create the service
    let service = TokiService::new()?;

    // Run the server with stdio transport
    let server = service.serve(stdio()).await?;

    // Wait for the server to finish
    server.waiting().await?;

    log::info!("Toki MCP server stopped");
    Ok(())
}
