# ding

A multi-provider notification CLI for development workflows. Send notifications from local or remote sessions, with native macOS support and integrations for Claude Code and OpenAI Codex.

## Features

- **Native macOS notifications** via Notification Center
- **Remote delivery** via HTTP to receive notifications from SSH sessions
- **Telegram notifications** via bot token + chat ID
- **Claude Code integration** with hook-based event handling
- **OpenAI Codex integration** for CLI notifications
- **Context-aware click handling** that returns focus to your terminal/tmux pane
- **Configuration-driven** with TOML-based settings

## Installation

### From source

```bash
git clone https://github.com/fcoury/ding.git
cd ding
cargo install --path .
```

### Requirements

- Rust 1.70+
- macOS 10.13+ (for Notification Center support)

## Quick Start

```bash
# Send a simple notification
ding send "Build complete"

# With title and sound
ding send "All tests passed" --title "Tests" --sound default

# Wait for click before continuing
ding send "Review needed" --wait-for-click

# Execute command on click
ding send "Deploy ready" --on-click "open https://example.com"
```

## Configuration

Create a config file:

```bash
ding config init
```

Config location: `~/.config/ding/config.toml`

```toml
default_provider = "macos"

[macos]
sound = "default"

[remote]
host = "192.168.1.100"
port = 4280
token = "your-secret-token"
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
icon = "~/.config/ding/icons/claude.icns"

[sources.codex]
icon = "~/.config/ding/icons/openai.icns"
```

## Commands

| Command | Description |
|---------|-------------|
| `send <MESSAGE>` | Send a notification |
| `config init` | Create default config file |
| `config set <KEY> <VALUE>` | Set a config value |
| `config list` | Display current config |
| `config path` | Show config file location |
| `listen` | Start HTTP listener for remote notifications |
| `remote ping` | Test connection to remote listener |
| `remote forward {on\|off\|toggle\|status}` | Manage remote forwarding |
| `telegram chat-id` | Fetch Telegram chat IDs |
| `install {claude\|codex}` | Show integration setup |
| `hook {claude\|codex}` | Process hook events |
| `focus` | Restore terminal focus |
| `providers list` | List available providers |
| `sources list` | List configured sources |

## Send Options

```
--title <TITLE>        Notification title
--icon <PATH>          Custom icon path
--no-icon              Disable icon
--link <URL>           URL to open on click
--sound <SOUND>        Sound name (or "default")
--silent               No sound
--urgency <LEVEL>      low, normal, or high
--tag <TAG>            Category/group tag
--source <SOURCE>      Source identifier (claude, codex, etc.)
--on-click <CMD>       Command to run when clicked
--wait-for-click       Block until notification is clicked
--background           Detach and wait in background
--json                 Output JSON result
--provider <NAME>      Override provider (macos, remote, telegram)

Telegram options:
--telegram-token <TOKEN>
--telegram-chat-id <CHAT_ID>
--telegram-parse-mode <MODE>
--telegram-silent

Remote options:
--remote-host <HOST>
--remote-port <PORT>
--remote-token <TOKEN>
--remote-timeout-ms <MS>
--remote-retries <N>
--no-fallback
```

## Remote Usage

### On your local machine (receiver)

Start the listener:

```bash
ding listen --token "your-secret-token"
```

### On a remote server (sender)

Configure remote delivery:

```bash
ding config set remote.host "your-local-ip"
ding config set remote.port 4280
ding config set remote.token "your-secret-token"
```

Enable remote forwarding:

```bash
ding remote forward on --host YOUR_LOCAL_IP --port 4280
```

Now notifications from the remote server appear on your local machine:

```bash
ding send "Remote build complete"
```

## Claude Code Integration

### Setup

```bash
# View installation instructions
ding install claude

# Apply the integration
ding install claude --apply
```

This adds a hook to `~/.claude/settings.json`:

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

When Claude Code emits events, ding receives them and shows appropriate notifications:

- **High urgency**: Permission prompts, failures, auth issues
- **Normal urgency**: Task completion, file changes
- **Low urgency**: Progress updates, plan changes

Click a notification to return focus to your Claude Code session.

## OpenAI Codex Integration

### Setup

```bash
# View installation instructions
ding install codex

# Apply the integration
ding install codex --apply
```

Works the same as Claude Code integration, processing Codex CLI events into notifications.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DING_TERMINAL_APP` | Override detected terminal app |
| `DING_TMUX_SESSION` | Override tmux session |
| `DING_TMUX_WINDOW` | Override tmux window |
| `DING_TMUX_PANE` | Override tmux pane |

## Click Handler Environment

When `--on-click` runs, these environment variables are set:

| Variable | Description |
|----------|-------------|
| `DING_SOURCE` | Notification source |
| `DING_TITLE` | Notification title |
| `DING_MESSAGE` | Notification message |
| `DING_TAG` | Notification tag |
| `DING_TMUX_SESSION` | Originating tmux session |
| `DING_TMUX_WINDOW` | Originating tmux window |
| `DING_TMUX_PANE` | Originating tmux pane |
| `DING_TERMINAL_APP` | Originating terminal app |
| `DING_CONTEXT_JSON` | Full context as JSON |

## Listener Security

The listener supports:

- **Token authentication**: `--token` and `--require-token`
- **Host allowlist**: `--allow-host` to restrict by IP/hostname

```bash
ding listen \
  --token "secret" \
  --require-token \
  --allow-host 192.168.1.0/24
```

## Examples

### Build notifications

```bash
cargo build && ding send "Build succeeded" --sound default || ding send "Build failed" --urgency high
```

### Long-running task

```bash
./long-task.sh && ding send "Task complete" --wait-for-click --on-click "ding focus"
```

### Remote SSH workflow

```bash
# Local: start listener
ding listen

# Remote: enable forwarding and send
ding remote forward on --host 192.168.1.100 --port 4280
ding send "Deployment complete"
```

### Background notification

```bash
ding send "Review when ready" --background --on-click "open https://github.com/pr/123"
```

## License

MIT
## Telegram Usage

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
ding send "Hello from ding" --provider telegram
```
