# Toki

**Automatic time tracking for developers** - Track your work without thinking about it.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

---

## The Problem

Time tracking is tedious. You forget to start timers, forget to stop them, and end up guessing at the end of the day. Traditional tools require constant manual input, breaking your flow.

## The Solution

Toki runs silently in the background, automatically detecting:
- Which project you're working on (from IDE window titles)
- Which issue you're addressing (from git branch names)
- How you spend your time (AI-powered activity classification)

No buttons to click. No timers to start. Just work.

---

## Features

### Quiet Technology
- **Zero-friction tracking** - Runs as a background daemon
- **Automatic project detection** - Parses IDE window titles (VS Code, Cursor, etc.)
- **Git branch to issue linking** - Extracts issue IDs from branch names (e.g., `feature/PROJ-123`)

### AI-Powered Classification
- **Semantic Gravity** - Uses local embeddings to classify activities by relevance
- **Smart issue matching** - Suggests related issues based on your work context
- **No explicit rules needed** - AI learns from patterns, not configurations

### Privacy-First
- **100% local** - All data stored in SQLite on your machine
- **No cloud sync** - Unless you explicitly configure it
- **App exclusion** - Hide sensitive applications from tracking

### PM System Integration
- **Plane.so** - Sync time entries to your project management system
- **Notion** - Use Notion databases as issue sources with time tracking
- **More coming** - GitHub, Jira, Linear (planned)

---

## Quick Start

### Installation

```bash
# Build from source
cargo build --release

# Install binary
sudo cp target/release/toki /usr/local/bin/

# Initialize configuration
toki init
```

See [INSTALL.md](INSTALL.md) for detailed instructions including system service setup.

### Usage

```bash
# Start the daemon
toki start

# Check what's being tracked
toki status

# View today's activity
toki report today

# Review and link activities to issues
toki review

# Stop the daemon
toki stop
```

### Plane.so Integration

```bash
# Configure API access
toki config set plane.api_key <your-api-key>
toki config set plane.workspace <workspace-slug>

# Link a local project to a Plane project
toki project link --project <local-project> --plane-project <IDENTIFIER>

# Sync issues for AI matching
toki issue-sync

# Sync time entries
toki sync plane
```

### Notion Integration

Toki can use Notion databases as issue sources with automatic time tracking support.

```bash
# 1. Create a Notion Integration
#    Go to https://www.notion.so/my-integrations
#    Create a new integration and copy the token

# 2. Configure API access
toki config set notion.api_key <your-integration-token>

# 3. Connect your database to the integration
#    In Notion: Open database → ... menu → Add connections → Your integration

# 4. Test connection and list databases
toki notion test
toki notion databases

# 5. View database schema (shows detected property mappings)
toki notion pages --database <database-id> --schema

# 6. Link a local project to a Notion database
toki project link --project <local-project> --notion-database <database-id>

# 7. Sync issues for AI matching
toki issue-sync
```

#### Automatic Property Detection

Toki automatically detects property mappings from your Notion database:

| Role | Detected Property Names |
|------|-------------------------|
| Title | Name, Title, Task, 名稱, タイトル |
| Status | Status, State, 狀態, ステータス |
| Description | Description, Notes, 描述, 説明 |
| Time | Time, Hours, Duration, 時間, 工時 |

You can override automatic detection:
```bash
toki config set notion.time_property "Hours Spent"
```

### Notion to GitHub/GitLab Sync

Sync Notion database pages to GitHub or GitLab issues:

```bash
# Configure GitHub token
toki config set github.token <your-github-pat>

# Configure GitLab token (for gitlab.com)
toki config set gitlab.token <your-gitlab-pat>

# For self-hosted GitLab
toki config set gitlab.api_url https://gitlab.your-company.com

# Sync Notion pages to GitHub issues
toki notion sync-to-github --database <database-id> --repo owner/repo

# Sync Notion pages to GitLab issues
toki notion sync-to-gitlab --database <database-id> --project group/project

# Check sync status
toki notion sync-status --database <database-id>

# Dry-run mode (preview without creating issues)
toki notion sync-to-github --database <database-id> --repo owner/repo --dry-run
```

### MCP Server (for AI Agents)

Toki includes an MCP (Model Context Protocol) server for AI agents like Claude Desktop:

```bash
# Build the MCP server
cargo build --release -p toki-mcp

# Run the MCP server (uses stdio transport)
./target/release/toki-mcp
```

#### Claude Desktop Configuration

Add to your `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "toki": {
      "command": "/path/to/toki-mcp",
      "args": []
    }
  }
}
```

#### Available MCP Tools

| Tool | Description |
|------|-------------|
| `notion_list_databases` | List all accessible Notion databases |
| `notion_list_pages` | List pages in a Notion database |
| `notion_sync_to_github` | Sync Notion pages to GitHub Issues |
| `notion_sync_to_gitlab` | Sync Notion pages to GitLab Issues |
| `notion_sync_status` | Show sync history for a database |
| `project_list` | List tracked projects |
| `config_get` | Get a configuration value |
| `config_set` | Set a configuration value |

---

## Architecture

```
toki/
├── crates/
│   ├── toki-cli/           # Command-line interface
│   ├── toki-core/          # Daemon: polling, session management, IPC
│   ├── toki-storage/       # SQLite database, models, migrations
│   ├── toki-ai/            # Semantic gravity, embeddings, issue matching
│   ├── toki-detector/      # Git parsing, IDE workspace detection
│   ├── toki-integrations/  # Plane.so, Notion, GitHub, GitLab APIs
│   └── toki-mcp/           # MCP server for AI agent integration
└── contrib/
    ├── toki.plist          # macOS launchd service
    └── toki.service        # Linux systemd service
```

### Data Flow

```
System Monitor (active app, window title)
        │
        v
Work Context Detector (project path, git branch)
        │
        v
AI Classifier (semantic gravity scoring)
        │
        v
ActivitySpan → SQLite Database
        │
        v
CLI queries via Unix Socket IPC
```

---

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Lint (strict - must pass with 0 errors/warnings)
cargo clippy

# Run daemon in foreground for debugging
RUST_LOG=debug cargo run -- start --foreground --interval 1
```

### Code Quality Standards

- `#![forbid(unsafe_code)]` in all crates except `toki-core` (platform APIs)
- Zero warnings policy
- Clippy pedantic mode enabled

---

## Configuration

Data is stored in `~/.toki/`:

| File | Description |
|------|-------------|
| `toki.db` | SQLite database |
| `toki.log` | Daemon logs |
| `toki.sock` | IPC socket (runtime) |
| `config.toml` | User configuration |

### Environment Variables

```bash
RUST_LOG=debug      # Log level: debug, info, warn, error
TOKI_DB_PATH=...    # Custom database path
```

---

## Roadmap

- [x] Automatic project detection
- [x] Git branch issue extraction
- [x] Plane.so integration
- [x] AI semantic gravity classification
- [x] Local embedding-based issue matching
- [x] Notion database integration
- [x] GitHub Issues integration
- [x] GitLab Issues integration (including self-hosted)
- [x] MCP server for AI agents
- [ ] Jira integration
- [ ] Linear integration
- [ ] Web dashboard
- [ ] Team sync (optional)

---

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

## About RikaiDev

**Rikai** (理解) means "understanding" in Japanese. We build tools that help developers understand their work through AI assistance.

- [Cortex AI](https://github.com/RikaiDev/cortex-ai) - AI collaboration brain for coding assistants
- [Toki](https://github.com/RikaiDev/toki) - Automatic time tracking

---

Built with Rust, SQLite, and local AI embeddings.
