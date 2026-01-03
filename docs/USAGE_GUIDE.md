# Ding Usage Guide

This guide covers detailed setup and usage of ding for local notifications, remote delivery, and integrations with Claude Code and OpenAI Codex.

## Table of Contents

1. [Installation](#installation)
2. [Basic Local Usage](#basic-local-usage)
3. [Configuration](#configuration)
4. [Claude Code Integration](#claude-code-integration)
5. [OpenAI Codex Integration](#openai-codex-integration)
6. [Remote Notifications](#remote-notifications)
7. [Advanced Usage](#advanced-usage)
8. [Troubleshooting](#troubleshooting)

---

## Installation

### Build from source

```bash
git clone https://github.com/fcoury/ding.git
cd ding
cargo build --release
```

The binary will be at `./target/release/ding`.

### Install to PATH

```bash
cargo install --path .
```

This installs to `~/.cargo/bin/ding`. Ensure `~/.cargo/bin` is in your PATH.

### Verify installation

```bash
ding --version
ding send "Hello from ding!"
```

---

## Basic Local Usage

### Simple notifications

```bash
# Basic message
ding send "Build complete"

# With title
ding send "All 42 tests passed" --title "Test Results"

# With sound
ding send "Deployment finished" --sound default

# Silent notification
ding send "Background task done" --silent
```

### Urgency levels

```bash
# Low urgency (subtle)
ding send "Sync complete" --urgency low

# Normal urgency (default)
ding send "Build finished" --urgency normal

# High urgency (prominent)
ding send "Build failed!" --urgency high
```

### Tags for grouping

```bash
ding send "Test 1 passed" --tag tests
ding send "Test 2 passed" --tag tests
ding send "Test 3 failed" --tag tests --urgency high
```

### Wait for interaction

```bash
# Block until notification is clicked
ding send "Review required" --wait-for-click
echo "User clicked the notification"

# Run command on click
ding send "PR ready for review" --on-click "open https://github.com/repo/pull/123"

# Background wait (doesn't block)
ding send "Check when ready" --background --on-click "ding focus"
```

### JSON output

```bash
ding send "Test" --json
# Output: {"delivered":true,"clicked":false,"action":null}
```

---

## Configuration

### Initialize config

```bash
ding config init
```

Creates `~/.config/ding/config.toml`.

### View config

```bash
# Show config file path
ding config path

# Display full config
ding config list
```

### Set config values

```bash
# Set default sound
ding config set macos.sound default

# Set remote host
ding config set remote.host 192.168.1.100

# Set remote port
ding config set remote.port 4280

# Set listener token
ding config set listener.token "your-secret-token"

# Set Telegram bot token
ding config set telegram.bot_token "123456:ABC..."

# Set Telegram chat id
ding config set telegram.chat_id "123456789"
```

### Full config example

```toml
# ~/.config/ding/config.toml

default_provider = "macos"

[macos]
sound = "default"
# app_bundle_id = "com.apple.Terminal"

[remote]
host = "192.168.1.100"
port = 4280
token = "your-secret-token"
timeout_ms = 2000
retries = 2
fallback_to_local = true

[listener]
bind = "0.0.0.0"
port = 4280
token = "your-secret-token"
require_token = true
prefix_hostname = true

[telegram]
bot_token = "123456:ABC..."
chat_id = "123456789"
parse_mode = "MarkdownV2"
silent = false

[sources.claude]
# Custom icon for Claude notifications
# icon = "/path/to/claude.icns"

[sources.codex]
# Custom icon for Codex notifications
# icon = "/path/to/openai.icns"
```

---

## Claude Code Integration

Claude Code is Anthropic's AI coding assistant. Ding can receive hook events from Claude Code and display native notifications.

### Step 1: View installation instructions

```bash
ding install claude
```

This shows what changes will be made to your Claude Code configuration.

### Step 2: Apply the integration

```bash
ding install claude --apply
```

This modifies `~/.claude/settings.json` to add notification hooks.

### Step 3: Verify setup

The integration adds this to your Claude settings:

```json
{
  "hooks": {
    "Notification": [
      {
        "matcher": "",
        "hooks": ["ding hook claude"]
      }
    ]
  }
}
```

### How it works

1. Claude Code emits events during operation (task complete, permission needed, etc.)
2. Events are sent to `ding hook claude` via stdin as JSON
3. Ding parses the event and shows an appropriate notification
4. Clicking the notification returns focus to your terminal/tmux session

### Event urgency mapping

| Event Type | Urgency |
|------------|---------|
| Permission prompts | High |
| Failures/errors | High |
| Auth issues | High |
| Task completion | Normal |
| File changes | Normal |
| Progress updates | Low |
| Plan changes | Low |

### Manual hook testing

```bash
echo '{"type":"task_complete","message":"Refactoring done"}' | ding hook claude
```

### Using with remote sessions

If you're running Claude Code over SSH, enable remote forwarding:

```bash
# On your local machine
ding listen

# On the remote server
ding remote forward on --host YOUR_LOCAL_IP --port 4280
```

Now Claude Code notifications from the remote session appear on your local machine.

---

## OpenAI Codex Integration

OpenAI Codex CLI is another AI coding assistant. Ding provides similar integration.

### Step 1: View installation instructions

```bash
ding install codex
```

### Step 2: Apply the integration

```bash
ding install codex --apply
```

### Step 3: Usage

Works identically to Claude Code integration. Events from Codex CLI are processed and displayed as native notifications.

### Manual hook testing

```bash
echo '{"type":"completion","message":"Code generated"}' | ding hook codex
```

---

## Remote Notifications

Remote notifications let you receive notifications on your local machine from commands running on remote servers (via SSH).

### Architecture

```
┌─────────────────┐         HTTP POST         ┌─────────────────┐
│  Remote Server  │  ──────────────────────►  │  Local Machine  │
│                 │                           │                 │
│  ding send   │   :4280/notify            │  ding listen │
│  (remote mode)  │                           │  (HTTP server)  │
└─────────────────┘                           └─────────────────┘
                                                      │
                                                      ▼
                                              ┌─────────────────┐
                                              │ macOS Notif.    │
                                              │ Center          │
                                              └─────────────────┘
```

### Local machine setup (receiver)

#### 1. Configure the listener

```bash
# Set auth token
ding config set listener.token "your-secret-token"
ding config set listener.require_token true
ding config set listener.bind "0.0.0.0"
ding config set listener.port 4280
```

#### 2. Start the listener

```bash
ding listen
```

Or with command-line options:

```bash
ding listen \
  --bind 0.0.0.0 \
  --port 4280 \
  --token "your-secret-token" \
  --require-token
```

#### 3. Keep listener running

Use a process manager or terminal multiplexer:

```bash
# With tmux
tmux new-session -d -s ding 'ding listen'

# With launchd (macOS)
# Create ~/Library/LaunchAgents/com.ding.listener.plist
```

### Remote server setup (sender)

#### 1. Install ding on remote

```bash
# SSH to remote server
ssh user@remote-server

# Install ding
git clone https://github.com/fcoury/ding.git
cd ding
cargo install --path .
```

#### 2. Configure remote delivery

```bash
ding config set remote.host "YOUR_LOCAL_IP"
ding config set remote.port 4280
ding config set remote.token "your-secret-token"
```

#### 3. Enable remote forwarding

```bash
ding remote forward on --host YOUR_LOCAL_IP --port 4280
```

Check status:

```bash
ding remote forward status
```

#### 4. Test connection

```bash
ding remote ping
```

#### 5. Send notifications

```bash
ding send "Remote build complete"
```

The notification appears on your local machine with `[hostname]` prefix.

### Security considerations

#### Token authentication

Always use a token in production:

```bash
# Local
ding listen --token "secret" --require-token

# Remote
ding config set remote.token "secret"
```

#### Host allowlist

Restrict which hosts can send notifications:

```bash
ding listen --allow-host 192.168.1.0/24 --allow-host 10.0.0.5
```

#### Firewall

Ensure port 4280 (or your chosen port) is accessible from remote servers but not the public internet.

### Fallback behavior

If remote delivery fails, ding can fall back to local notifications:

```bash
# Enable fallback (default)
ding config set remote.fallback_to_local true

# Disable fallback
ding send "Must reach remote" --no-fallback
```

---

## Telegram Notifications

### Configure Telegram

```bash
ding config set telegram.bot_token "123456:ABC..."
ding config set telegram.chat_id "123456789"
ding config set telegram.parse_mode "MarkdownV2"
ding config set telegram.silent false
```

### Fetch chat IDs

```bash
ding telegram chat-id --token "123456:ABC..."
```

Set the first ID into config:

```bash
ding telegram chat-id --token "123456:ABC..." --apply
```

### Send a Telegram notification

```bash
ding send "Build complete" --provider telegram
```

### Override settings per message

```bash
ding send "Deploy ready" --provider telegram \
  --telegram-token "123456:ABC..." \
  --telegram-chat-id "123456789" \
  --telegram-parse-mode MarkdownV2
```

---

## Advanced Usage

### Context-aware click handling

When you click a notification, ding can return focus to the originating terminal:

```bash
ding send "Click to return" --on-click "ding focus"
```

The focus command uses captured context:

- **Terminal app**: iTerm, Ghostty, Terminal, etc.
- **Tmux session/window/pane**: Restores exact pane

### Custom click commands

```bash
# Open URL
ding send "PR merged" --on-click "open https://github.com/repo/pull/123"

# Run script
ding send "Deploy ready" --on-click "./scripts/deploy.sh"

# Multiple commands
ding send "Review" --on-click "ding focus && echo 'Focused'"
```

### Environment in click handlers

Click commands receive context via environment variables:

```bash
ding send "Test" --source myapp --on-click 'echo $DING_SOURCE'
# Outputs: myapp
```

Available variables:

- `DING_SOURCE`
- `DING_TITLE`
- `DING_MESSAGE`
- `DING_TAG`
- `DING_TMUX_SESSION`
- `DING_TMUX_WINDOW`
- `DING_TMUX_PANE`
- `DING_TERMINAL_APP`
- `DING_CONTEXT_JSON`

### Build workflow integration

```bash
#!/bin/bash
# build-notify.sh

if cargo build --release; then
    ding send "Build succeeded" \
        --title "Cargo Build" \
        --tag build \
        --sound default
else
    ding send "Build failed" \
        --title "Cargo Build" \
        --tag build \
        --urgency high \
        --wait-for-click
fi
```

### Long-running task wrapper

```bash
#!/bin/bash
# notify-on-complete.sh

"$@"
exit_code=$?

if [ $exit_code -eq 0 ]; then
    ding send "Command succeeded: $1" --sound default
else
    ding send "Command failed: $1 (exit $exit_code)" --urgency high
fi

exit $exit_code
```

Usage:

```bash
./notify-on-complete.sh make test
```

### Provider override

Force a specific provider:

```bash
# Always use local macOS
ding send "Local only" --provider macos

# Always use remote
ding send "Remote only" --provider remote
```

---

## Troubleshooting

### Notifications not appearing

1. Check macOS notification settings:
   - System Preferences → Notifications → Terminal (or your terminal app)
   - Ensure notifications are enabled

2. Test basic notification:
   ```bash
   ding send "Test notification"
   ```

3. Check if Do Not Disturb is enabled

### Remote notifications not working

1. Test connectivity:
   ```bash
   ding remote ping
   ```

2. Check listener is running:
   ```bash
   curl http://YOUR_LOCAL_IP:4280/health
   ```

3. Verify token matches on both sides

4. Check firewall allows port 4280

5. Verify remote forwarding is enabled:
   ```bash
   ding remote forward status
   ```

### Click handler not working

1. Test the command directly:
   ```bash
   ding focus
   ```

2. Check tmux context is captured:
   ```bash
   ding send "Test" --json --on-click "env | grep DING"
   ```

3. For background mode, check the process is running:
   ```bash
   ps aux | grep ding
   ```

### Claude/Codex integration issues

1. Verify hook is installed:
   ```bash
   cat ~/.claude/settings.json | grep ding
   ```

2. Test hook manually:
   ```bash
   echo '{"type":"test"}' | ding hook claude
   ```

3. Check ding is in PATH for the hook

### Sound not playing

1. Check system volume
2. Verify sound name is valid:
   ```bash
   # List available sounds
   ls /System/Library/Sounds/
   ```
3. Use `--sound default` for the default notification sound

### Config not loading

1. Check config path:
   ```bash
   ding config path
   ```

2. Validate config syntax:
   ```bash
   ding config list
   ```

3. Check file permissions on config file

---

## Summary

| Use Case | Command |
|----------|---------|
| Simple notification | `ding send "Message"` |
| With sound | `ding send "Done" --sound default` |
| Wait for click | `ding send "Review" --wait-for-click` |
| Run on click | `ding send "Open" --on-click "open URL"` |
| Remote setup | `ding listen` (local) + `ding remote forward on --host <host> --port 4280` (remote) |
| Telegram notify | `ding send "Message" --provider telegram` |
| Claude integration | `ding install claude --apply` |
| Codex integration | `ding install codex --apply` |
| Check config | `ding config list` |
| Test remote | `ding remote ping` |
