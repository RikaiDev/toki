# Toki

**Automatic time tracking for developers** - Track your work without thinking about it.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue.svg)]()

---

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| macOS (Apple Silicon) | ✅ Supported | Primary development platform |
| macOS (Intel) | ✅ Supported | |
| Linux (x64) | ✅ Supported | |
| Linux (ARM64) | ✅ Supported | |
| Windows | ❌ Not supported | Uses Unix sockets for IPC. Use WSL2 instead. |

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

### AI-Powered Intelligence
- **Semantic Gravity** - Uses local embeddings to classify activities by relevance
- **Smart issue matching** - Suggests related issues based on your work context
- **Time estimation** - AI-powered estimates based on complexity and historical data
- **Next task suggestion** - Recommends what to work on based on context and constraints
- **No explicit rules needed** - AI learns from patterns, not configurations

### Claude Code Integration
- **Automatic session tracking** - Hooks into Claude Code for seamless tracking
- **Outcome recording** - Tracks commits, issues opened/closed, PRs created
- **Multi-issue sessions** - Link multiple issues to a single coding session
- **Work context awareness** - Understands what you're working on from git state

### Productivity Insights
- **Anomaly detection** - Identifies unusual patterns in your work
- **Peak hours analysis** - Find your most productive times
- **Context switch tracking** - Monitor focus fragmentation
- **Actionable suggestions** - Get personalized productivity tips

### Reports & Summaries
- **Standup reports** - Auto-generate yesterday/today/blockers format
- **Work summaries** - AI-powered narrative of your accomplishments
- **Multiple formats** - Text, Markdown, Slack, Discord, Teams, JSON

### Privacy-First
- **100% local** - All data stored in SQLite on your machine
- **No cloud sync** - Unless you explicitly configure it
- **App exclusion** - Hide sensitive applications from tracking

### PM System Integration
- **Plane.so** - Sync time entries to your project management system
- **Notion** - Use Notion databases as issue sources with time tracking
- **GitHub/GitLab** - Sync issues and track time against them

---

## Quick Start

### Installation

#### Option 1: Download from Releases (Recommended)

Download the latest release from [GitHub Releases](https://github.com/RikaiDev/toki/releases):

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/RikaiDev/toki/releases/latest/download/toki-cli-aarch64-apple-darwin.tar.xz
tar -xf toki-cli-aarch64-apple-darwin.tar.xz
sudo cp toki-cli-aarch64-apple-darwin/toki /usr/local/bin/

# macOS (Intel)
curl -LO https://github.com/RikaiDev/toki/releases/latest/download/toki-cli-x86_64-apple-darwin.tar.xz
tar -xf toki-cli-x86_64-apple-darwin.tar.xz
sudo cp toki-cli-x86_64-apple-darwin/toki /usr/local/bin/

# Linux (x64)
curl -LO https://github.com/RikaiDev/toki/releases/latest/download/toki-cli-x86_64-unknown-linux-gnu.tar.xz
tar -xf toki-cli-x86_64-unknown-linux-gnu.tar.xz
sudo cp toki-cli-x86_64-unknown-linux-gnu/toki /usr/local/bin/
```

#### Option 2: Build from Source

```bash
cargo build --release
sudo cp target/release/toki /usr/local/bin/
```

#### Initialize

```bash
toki init
```

See [INSTALL.md](INSTALL.md) for detailed instructions including system service setup.

### Basic Usage

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

### AI-Powered Features

```bash
# Estimate time for an issue
toki estimate 123                    # By issue number
toki estimate PROJ-123 --system github

# Get next task suggestion
toki next                            # Default suggestions
toki next --time 30m --focus low     # With constraints
toki next --time 2h --focus deep     # Deep work mode

# Suggest issues from current work context
toki suggest-issue                   # From current directory
toki suggest-issue --apply           # Auto-link best match
```

### Reports & Insights

```bash
# Generate standup report
toki standup                         # Text format
toki standup --format slack          # Slack-formatted
toki standup --format json           # JSON for automation

# Generate work summary
toki summary generate                # AI-powered narrative
toki summary generate --format json

# View productivity insights
toki insights                        # Weekly summary
toki insights --period month         # Monthly analysis
toki insights --compare              # Compare with previous period
toki insights --focus sessions       # Focus on session patterns
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

### Claude Code Integration

Toki integrates with [Claude Code](https://claude.com/claude-code) via hooks for automatic session tracking.

#### Setup Hooks

```bash
# Create hooks directory
mkdir -p ~/.claude/hooks

# Create the hooks configuration
cat > ~/.claude/hooks.json << 'EOF'
{
  "hooks": [
    {
      "event": "PostToolUse",
      "script": "~/.claude/hooks/post_tool_use.sh"
    }
  ]
}
EOF

# Create the post-tool-use hook
cat > ~/.claude/hooks/post_tool_use.sh << 'EOF'
#!/bin/bash
# Auto-record outcomes to toki

TOOL_NAME=$(echo "$CLAUDE_TOOL_USE" | jq -r '.tool_name // empty')
TOOL_INPUT=$(echo "$CLAUDE_TOOL_USE" | jq -r '.tool_input // empty')
SESSION_ID="$CLAUDE_SESSION_ID"
CWD="$CLAUDE_WORKING_DIRECTORY"

case "$TOOL_NAME" in
  "Bash")
    CMD=$(echo "$TOOL_INPUT" | jq -r '.command // empty')

    # Detect git commits
    if [[ "$CMD" =~ ^git\ commit ]]; then
      MSG=$(echo "$CMD" | grep -oP '(?<=-m ["\x27])[^"\x27]+')
      toki session record-outcome "$SESSION_ID" commit --ref "HEAD" --metadata "{\"message\":\"$MSG\"}" 2>/dev/null
    fi

    # Detect gh issue operations
    if [[ "$CMD" =~ ^gh\ issue\ create ]]; then
      toki session record-outcome "$SESSION_ID" issue_opened 2>/dev/null
    elif [[ "$CMD" =~ ^gh\ issue\ close ]]; then
      ISSUE=$(echo "$CMD" | grep -oP '\d+')
      toki session record-outcome "$SESSION_ID" issue_closed --ref "$ISSUE" 2>/dev/null
    fi

    # Detect gh pr create
    if [[ "$CMD" =~ ^gh\ pr\ create ]]; then
      toki session record-outcome "$SESSION_ID" pr_opened 2>/dev/null
    fi
    ;;
esac
EOF

chmod +x ~/.claude/hooks/post_tool_use.sh
```

#### Session Commands

```bash
# Start a session (usually automatic via hooks)
toki session start <session-id> --cwd /path/to/project

# Link issues to current session
toki session link <session-id> --issue 123 --system github
toki session link <session-id> --issue PROJ-456 --system notion

# Record outcomes
toki session record-outcome <session-id> commit --ref abc123
toki session record-outcome <session-id> issue_closed --ref 42

# End session
toki session end <session-id>

# View session history
toki session list
toki session show <session-id>
```

### MCP Server (for AI Agents)

Toki includes an MCP (Model Context Protocol) server for AI agents. This allows AI assistants like Claude to directly interact with Toki's functionality.

#### Step 1: Build the MCP Server

```bash
# Clone the repository (if not already done)
git clone https://github.com/RikaiDev/toki.git
cd toki

# Build the MCP server
cargo build --release -p toki-mcp

# Verify the binary exists
ls -la target/release/toki-mcp
```

#### Step 2: Configure Your AI Platform

Choose your platform and follow the step-by-step instructions:

<details>
<summary><b>Claude Desktop (macOS)</b></summary>

1. **Locate the config file path:**
   ```bash
   # The config file location
   ~/Library/Application Support/Claude/claude_desktop_config.json
   ```

2. **Create or edit the config file:**
   ```bash
   # Create directory if it doesn't exist
   mkdir -p ~/Library/Application\ Support/Claude

   # Create/edit the config file
   nano ~/Library/Application\ Support/Claude/claude_desktop_config.json
   ```

3. **Add the toki MCP server configuration:**
   ```json
   {
     "mcpServers": {
       "toki": {
         "command": "/absolute/path/to/toki/target/release/toki-mcp",
         "args": []
       }
     }
   }
   ```
   > ⚠️ Replace `/absolute/path/to/toki` with your actual toki directory path

4. **Restart Claude Desktop:**
   - Quit Claude Desktop completely (Cmd+Q)
   - Reopen Claude Desktop

5. **Verify the connection:**
   - Click the 🔌 icon in Claude Desktop
   - You should see "toki" listed as connected

</details>

<details>
<summary><b>Claude Desktop (Windows)</b></summary>

1. **Locate the config file path:**
   ```
   %APPDATA%\Claude\claude_desktop_config.json
   ```

2. **Create or edit the config file:**
   - Open File Explorer
   - Navigate to `C:\Users\<YourUsername>\AppData\Roaming\Claude\`
   - Create `claude_desktop_config.json` if it doesn't exist

3. **Add the toki MCP server configuration:**
   ```json
   {
     "mcpServers": {
       "toki": {
         "command": "C:\\path\\to\\toki\\target\\release\\toki-mcp.exe",
         "args": []
       }
     }
   }
   ```
   > ⚠️ Use double backslashes `\\` in Windows paths

4. **Restart Claude Desktop:**
   - Close Claude Desktop from the system tray
   - Reopen Claude Desktop

5. **Verify the connection:**
   - Click the 🔌 icon in Claude Desktop
   - You should see "toki" listed as connected

</details>

<details>
<summary><b>Claude Desktop (Linux)</b></summary>

1. **Locate the config file path:**
   ```bash
   ~/.config/claude/claude_desktop_config.json
   ```

2. **Create or edit the config file:**
   ```bash
   # Create directory if it doesn't exist
   mkdir -p ~/.config/claude

   # Create/edit the config file
   nano ~/.config/claude/claude_desktop_config.json
   ```

3. **Add the toki MCP server configuration:**
   ```json
   {
     "mcpServers": {
       "toki": {
         "command": "/absolute/path/to/toki/target/release/toki-mcp",
         "args": []
       }
     }
   }
   ```

4. **Restart Claude Desktop:**
   - Close Claude Desktop
   - Reopen Claude Desktop

5. **Verify the connection:**
   - Click the 🔌 icon in Claude Desktop
   - You should see "toki" listed as connected

</details>

<details>
<summary><b>Claude Code (CLI)</b></summary>

1. **Add the MCP server using CLI command:**
   ```bash
   # Navigate to your toki project directory
   cd /path/to/toki

   # Add toki MCP server (project scope - shared via git)
   claude mcp add --transport stdio toki --scope project -- /absolute/path/to/toki/target/release/toki-mcp

   # Or add to user scope (available in all projects)
   claude mcp add --transport stdio toki --scope user -- /absolute/path/to/toki/target/release/toki-mcp
   ```

2. **Verify the configuration:**
   ```bash
   # Check MCP server status
   claude mcp list

   # Should show: toki: ... - ✓ Connected
   ```

3. **Manual configuration (alternative):**

   Create `.mcp.json` in your project root:
   ```json
   {
     "mcpServers": {
       "toki": {
         "type": "stdio",
         "command": "/absolute/path/to/toki/target/release/toki-mcp",
         "args": [],
         "env": {}
       }
     }
   }
   ```

4. **Restart Claude Code:**
   ```bash
   # Exit current session
   exit

   # Start new session
   claude
   ```

5. **Test the tools:**
   - Ask Claude: "List my Notion databases using toki"
   - Or: "Show toki projects"

</details>

#### Step 3: Configure Toki Integrations

Before using the MCP tools, configure your API keys:

```bash
# Using toki CLI
toki config set notion.api_key <your-notion-integration-token>
toki config set github.token <your-github-pat>
toki config set gitlab.token <your-gitlab-pat>

# Or ask the AI to configure via MCP
# "Set my Notion API key to ntn_xxx..."
```

#### Step 4: Test the MCP Tools

Try these commands with your AI assistant:

| Request | MCP Tool Used |
|---------|---------------|
| "List my Notion databases" | `notion_list_databases` |
| "Show pages in database abc123" | `notion_list_pages` |
| "Sync Notion to GitHub repo owner/repo" | `notion_sync_to_github` |
| "Show sync status for database abc123" | `notion_sync_status` |
| "List toki projects" | `project_list` |
| "Get my Notion API key" | `config_get` |

#### Available MCP Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| `notion_list_databases` | List all accessible Notion databases | None |
| `notion_list_pages` | List pages in a Notion database | `database_id` |
| `notion_sync_to_github` | Sync Notion pages to GitHub Issues | `database_id`, `repo` |
| `notion_sync_to_gitlab` | Sync Notion pages to GitLab Issues | `database_id`, `project` |
| `notion_sync_status` | Show sync history for a database | `database_id` |
| `project_list` | List tracked projects | None |
| `config_get` | Get a configuration value | `key` |
| `config_set` | Set a configuration value | `key`, `value` |

#### Troubleshooting

<details>
<summary><b>MCP server not connecting</b></summary>

1. **Check if the binary exists and is executable:**
   ```bash
   ls -la /path/to/toki/target/release/toki-mcp
   chmod +x /path/to/toki/target/release/toki-mcp
   ```

2. **Test the server manually:**
   ```bash
   /path/to/toki/target/release/toki-mcp
   # Should start without errors (Ctrl+C to stop)
   ```

3. **Check the path in config:**
   - Use absolute paths, not relative paths
   - No environment variables like `$HOME` or `~`

4. **Check logs:**
   - Claude Desktop: Check Console.app for errors
   - Claude Code: Run `claude mcp list` for status

</details>

<details>
<summary><b>Notion API errors</b></summary>

1. **Verify API key is set:**
   ```bash
   toki config get notion.api_key
   ```

2. **Test Notion connection:**
   ```bash
   toki notion test
   ```

3. **Ensure database is shared with integration:**
   - Open your Notion database
   - Click "..." menu → "Add connections"
   - Select your integration

</details>

<details>
<summary><b>GitHub/GitLab sync errors</b></summary>

1. **Verify tokens are set:**
   ```bash
   toki config get github.token
   toki config get gitlab.token
   ```

2. **Check token permissions:**
   - GitHub: Needs `repo` scope
   - GitLab: Needs `api` scope

3. **For self-hosted GitLab:**
   ```bash
   toki config set gitlab.api_url https://gitlab.your-company.com
   ```

</details>

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
┌─────────────────────────────────────────────────────────────┐
│                    Data Collection                          │
├─────────────────────────────────────────────────────────────┤
│  System Monitor          Claude Code Hooks                  │
│  (active app, window)    (session, outcomes)                │
│         │                       │                           │
│         v                       v                           │
│  Work Context Detector   Session Manager                    │
│  (project, git branch)   (commits, issues, PRs)             │
│         │                       │                           │
│         v                       v                           │
│  AI Classifier           Issue Linker                       │
│  (semantic gravity)      (multi-issue support)              │
│         │                       │                           │
│         └───────────┬───────────┘                           │
│                     v                                       │
│              SQLite Database                                │
└─────────────────────────────────────────────────────────────┘
                      │
                      v
┌─────────────────────────────────────────────────────────────┐
│                    AI Analysis                              │
├─────────────────────────────────────────────────────────────┤
│  Time Estimator    Next Task       Productivity Insights    │
│  (historical +     Suggester       (anomaly detection,      │
│   embeddings)      (scoring)        patterns)               │
└─────────────────────────────────────────────────────────────┘
                      │
                      v
┌─────────────────────────────────────────────────────────────┐
│                    Output                                   │
├─────────────────────────────────────────────────────────────┤
│  CLI Reports    Standup/Summary    PM Sync    MCP Server    │
│  (toki report)  (markdown/json)    (Notion,   (AI agents)   │
│                                     GitHub)                 │
└─────────────────────────────────────────────────────────────┘
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

### Completed
- [x] Automatic project detection
- [x] Git branch issue extraction
- [x] Plane.so integration
- [x] AI semantic gravity classification
- [x] Local embedding-based issue matching
- [x] Notion database integration
- [x] GitHub Issues integration
- [x] GitLab Issues integration (including self-hosted)
- [x] MCP server for AI agents
- [x] Claude Code hooks integration
- [x] Session outcome tracking (commits, issues, PRs)
- [x] Multi-issue session linking
- [x] AI-powered time estimation
- [x] Smart next task suggestion
- [x] Standup report generation
- [x] Work summary generation
- [x] Productivity insights & anomaly detection

### Future
- [ ] Jira integration
- [ ] Linear integration
- [ ] Web dashboard

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
