# Toki Installation Guide

## Quick Install (macOS/Linux)

```bash
# Build release binary
cargo build --release

# Install to system
sudo cp target/release/toki /usr/local/bin/

# Initialize
toki init

# Start daemon
toki start
```

## macOS - launchd Integration

### 1. Install Binary

```bash
cargo build --release
sudo cp target/release/toki /usr/local/bin/
sudo chmod +x /usr/local/bin/toki
```

### 2. Setup launchd Service

```bash
# Copy plist file
cp contrib/toki.plist ~/Library/LaunchAgents/com.user.toki.plist

# Edit the file to replace YOUR_USERNAME with your actual username
sed -i '' "s/YOUR_USERNAME/$(whoami)/g" ~/Library/LaunchAgents/com.user.toki.plist

# Load the service
launchctl load ~/Library/LaunchAgents/com.user.toki.plist

# Check status
launchctl list | grep toki
```

### 3. Control the Service

```bash
# Stop service
launchctl unload ~/Library/LaunchAgents/com.user.toki.plist

# Start service
launchctl load ~/Library/LaunchAgents/com.user.toki.plist

# Restart service
launchctl unload ~/Library/LaunchAgents/com.user.toki.plist
launchctl load ~/Library/LaunchAgents/com.user.toki.plist
```

## Linux - systemd Integration

### 1. Install Binary (Linux)

```bash
cargo build --release
sudo cp target/release/toki /usr/local/bin/
sudo chmod +x /usr/local/bin/toki
```

### 2. Setup systemd Service

```bash
# Copy service file (user service)
mkdir -p ~/.config/systemd/user
cp contrib/toki.service ~/.config/systemd/user/

# Reload systemd
systemctl --user daemon-reload

# Enable service (start on login)
systemctl --user enable toki.service

# Start service
systemctl --user start toki.service

# Check status
systemctl --user status toki.service
```

### 3. Control the Service (Linux)

```bash
# Stop service
systemctl --user stop toki.service

# Start service
systemctl --user start toki.service

# Restart service
systemctl --user restart toki.service

# View logs
journalctl --user -u toki.service -f
```

## Manual Daemon Control

You can also run Toki manually without system integration:

```bash
# Start daemon in background
toki start &

# Check status
toki status

# Stop daemon
toki stop
```

## Verify Installation

```bash
# Check binary location
which toki

# Check version/help
toki --help

# Initialize database
toki init

# Start tracking
toki start

# Check status
toki status

# View logs
tail -f ~/.toki/toki.log
```

## Uninstall

### macOS (launchd)

```bash
# Unload service
launchctl unload ~/Library/LaunchAgents/com.user.toki.plist

# Remove files
rm ~/Library/LaunchAgents/com.user.toki.plist
sudo rm /usr/local/bin/toki
rm -rf ~/.toki
```

### Linux (systemd)

```bash
# Stop and disable service
systemctl --user stop toki.service
systemctl --user disable toki.service

# Remove files
rm ~/.config/systemd/user/toki.service
systemctl --user daemon-reload
sudo rm /usr/local/bin/toki
rm -rf ~/.toki
```

## Configuration

Toki stores its data in `~/.toki/`:

- `~/.toki/toki.db` - SQLite database
- `~/.toki/toki.log` - Daemon logs
- `~/.toki/toki.sock` - IPC socket (when daemon is running)
- `~/.toki/encryption.key` - Encryption key (if enabled)

## Troubleshooting

### Daemon won't start

```bash
# Check logs
tail -f ~/.toki/toki.log

# Check if socket exists
ls -l ~/.toki/toki.sock

# Try running in foreground for debugging
toki start --interval 1
```

### Permission denied

```bash
# Make sure binary is executable
chmod +x /usr/local/bin/toki

# Check file permissions
ls -l ~/.toki/
```

### Service not starting on boot

```bash
# macOS
launchctl list | grep toki

# Linux
systemctl --user is-enabled toki.service
```

## Next Steps

After installation:

1. Initialize: `toki init`
2. Start daemon: `toki start`
3. Track work: `toki work-on PROJ-123`
4. Check status: `toki status`
5. View report: `toki report today`
