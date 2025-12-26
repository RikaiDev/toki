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
toki project link <local-project> <plane-project-id>

# Sync issues for AI matching
toki issue-sync

# Sync time entries
toki sync plane
```

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
│   └── toki-integrations/  # Plane.so API, webhooks
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
- [ ] GitHub Issues integration
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
