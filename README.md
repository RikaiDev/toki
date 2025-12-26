# Toki - Active Time Tracking for Software Engineers

Accurate, intelligent time tracking tool designed for software engineers.

## Features

### Phase 5 - Precise Time Tracking (Completed)

- **1-Second Precision**: Poll every second, accurately record each activity span
- **Smart Session Management**: Auto-detect work session start/end
- **ActivitySpan Model**: Accurately record duration for each app/task
- **Unix Socket IPC**: Real-time CLI-daemon communication
- **Daemon Management**: Complete control with start/stop/status/logs
- **System Integration**: Support for launchd (macOS) / systemd (Linux)
- **Graceful Shutdown**: SIGTERM/SIGINT signal handling

### Core Features

- **Automatic Tracking**: Monitor applications and window titles
- **Smart Classification**: Automatically categorize activities (work, meetings, breaks, etc.)
- **Work Item Binding**: Support for Plane.so, GitHub, Jira work item tracking
- **Privacy Protection**: Exclude sensitive apps, pause tracking
- **Report Generation**: Daily/weekly/monthly time statistics
- **PM System Sync**: Automatically sync work hours to project management systems

## Quick Start

### Installation

```bash
# Build
cargo build --release

# Install
sudo cp target/release/toki /usr/local/bin/

# Initialize
toki init
```

For detailed installation instructions, see [INSTALL.md](INSTALL.md)

### Basic Usage

```bash
# Start daemon
toki start

# Check status
toki status

# Track specific work item
toki work-on PROJ-123

# Stop tracking
toki work-off

# View reports
toki report today
toki report week

# Sync to Plane.so
toki sync plane

# Stop daemon
toki stop
```

## Architecture

### Time Tracking Precision

```text
Polling Interval: 1 second
Recording Unit: ActivitySpan (accurate to the second)
Session Management: Auto-detect work periods
Idle Detection: Configurable idle threshold
```

### Data Model

```text
Session (Work Period)
  ├── ActivitySpan (Activity Fragment, 1-second precision)
  │     ├── app_name
  │     ├── category
  │     ├── start_time
  │     ├── duration_seconds
  │     └── work_item_id (optional)
  └── WorkItem (Work Item)
        ├── issue_id (PROJ-123)
        ├── project
        └── accumulated_time
```

### Daemon Architecture

```text
CLI <─IPC Socket─> Daemon
                     │
                     ├── SessionManager (Session lifecycle)
                     ├── SystemMonitor (App monitoring)
                     ├── Classifier (Smart categorization)
                     ├── WorkContextDetector (Work item detection)
                     └── Database (SQLite)
```

## Project Structure

```text
toki/
├── crates/
│   ├── toki-core/          # Core daemon logic
│   │   ├── daemon.rs        # Main daemon (1-second polling)
│   │   ├── session_manager.rs  # Session lifecycle management
│   │   ├── ipc.rs           # Unix socket IPC
│   │   └── daemon_control.rs  # PID management, daemonize
│   ├── toki-storage/        # Data layer
│   │   ├── db.rs            # SQLite operations
│   │   ├── models.rs        # Data models
│   │   └── migrations.rs    # Schema migrations
│   ├── toki-ai/             # Smart analysis
│   │   ├── classifier.rs    # Activity classification
│   │   └── insights.rs      # Time analysis
│   ├── toki-detector/       # Work item detection
│   │   └── work_context.rs  # Detect work items from path/title
│   ├── toki-integrations/   # PM system integration
│   │   └── plane.rs         # Plane.so API
│   └── toki-cli/            # Command-line interface
│       └── main.rs          # CLI + IPC client
└── contrib/
    ├── toki.plist           # macOS launchd
    └── toki.service         # Linux systemd
```

## Development

### Build

```bash
cargo build
cargo test
cargo clippy
```

### Run Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# Foreground mode testing
RUST_LOG=debug cargo run -- start --interval 1
```

### Code Quality

- **0 errors, 0 warnings**: Strict linting rules
- **No unsafe code**: `#![forbid(unsafe_code)]`
- **Complete error handling**: Using `anyhow` and `thiserror`
- **Structured logging**: `log` + `env_logger`

## Configuration

### Data Directory

- **macOS/Linux**: `~/.toki/`
  - `toki.db` - SQLite database
  - `toki.log` - Daemon logs
  - `toki.sock` - IPC socket (when daemon is running)
  - `encryption.key` - Encryption key (optional)

### Environment Variables

```bash
# Log level
export RUST_LOG=info  # debug, info, warn, error

# Database path (optional)
export TOKI_DB_PATH=/custom/path/toki.db
```

## Privacy

- **Local First**: All data stored locally
- **Optional Encryption**: Support database encryption
- **App Exclusion**: Exclude sensitive applications
- **Pause Tracking**: Pause/resume tracking at any time
- **Data Export**: Support JSON/CSV export

## Integrations

### Plane.so

```bash
# Configure API
toki config set plane.api_url https://api.plane.so
toki config set plane.api_key your-api-key
toki config set plane.workspace my-workspace

# Sync work hours
toki sync plane
```

### GitHub (Planned)

```bash
toki config set github.token ghp_xxx
toki sync github
```

### Jira (Planned)

```bash
toki config set jira.url https://your-domain.atlassian.net
toki config set jira.token xxx
toki sync jira
```

## Contributing

Issues and Pull Requests are welcome!

### Development Workflow

1. Fork the project
2. Create feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open Pull Request

## License

MIT License - See [LICENSE](LICENSE) file for details

## Acknowledgments

- [Rust](https://www.rust-lang.org/) - Systems programming language
- [SQLite](https://www.sqlite.org/) - Embedded database
- [Tokio](https://tokio.rs/) - Async runtime
- [Plane.so](https://plane.so/) - Project management platform
