# Remote relay provider + listener plan

## Decisions

- Listener defaults to binding on all interfaces.
- Remote provider auto-fallbacks to local delivery if the listener is unreachable.
- Prefix titles with hostname by default.

## Goals

- Show notifications from SSH/remote agents on the local machine as if they were local.
- Keep the sender UX simple: `wakedev send` with a provider switch (or config default).
- Keep click handling local so focus returns to local terminal context.

## Architecture

- **Remote provider (client):** serializes a notification payload and POSTs to a local listener endpoint.
- **Listener (server):** receives payloads, validates, decorates, and re-emits via the local macOS provider.
- **Transport:** HTTP over localhost, typically via SSH port-forwarding.

## CLI

- `wakedev listen`
  - Starts an HTTP listener for incoming notifications.
  - Options: `--bind`, `--port`, `--token`, `--allow-host`, `--foreground/--daemon`, `--pidfile`.
  - Defaults: bind `0.0.0.0`, port `4280`.
- `wakedev send --provider remote`
  - Sends to listener (`remote.host`/`remote.port` or `--remote-host`/`--remote-port`).
  - Options: `--remote-host`, `--remote-port`, `--remote-token`, `--remote-timeout`, `--remote-retries`.
- `wakedev remote ping`
  - Health check endpoint to confirm listener connectivity.

## Config

```toml
[remote]
host = "127.0.0.1"
port = 4280
token = "..."
timeout_ms = 2000
retries = 2
fallback_to_local = true

[listener]
bind = "0.0.0.0"
port = 4280
token = "..."
require_token = true
prefix_hostname = true
```

## Payload

- JSON envelope:
  ```json
  {
    "notification": {
      "title": "...",
      "message": "...",
      "source": "...",
      "sound": "..."
    },
    "context": {
      "origin_host": "...",
      "origin_user": "...",
      "cwd": "...",
      "tmux": { "session": "...", "window": "...", "pane": "..." }
    }
  }
  ```
- Listener uses `context.origin_host` to prefix the title by default.

## Security

- Token-based auth in header (`Authorization: Bearer <token>` or `X-Wakedev-Token`).
- Listener defaults to binding on all interfaces and should be protected with token auth and allowlists.
- Optional allowlist for hostnames/IPs.

## Click handling

- Listener attaches `on_click` with local `wakedev focus` to restore the local context.
- Preserve incoming metadata so we can route to specific terminal panes later.

## Fallback behavior

- If remote delivery fails (timeout, refused, auth error), auto-fallback to local provider.
- Provide a `--no-fallback` flag to disable if needed.

## Implementation steps (proposed order)

1. **Shared payload types** in `src/remote.rs` (request/response + helpers).
2. **Listener command**: basic HTTP server, token auth, hostname prefixing, local delivery.
3. **Remote provider**: POST JSON, retry logic, fallback to local.
4. **Config + CLI flags** for remote/listener settings.
5. **Docs**: SSH port-forward example + config snippets.
6. **Tests** for payload validation + auth + fallback.

## SSH usage example

- Local machine:
  - `wakedev listen --bind 0.0.0.0 --port 4280`
- Remote machine:
  - `ssh -L 4280:127.0.0.1:4280 user@remote-host`
  - `wakedev send --provider remote --remote-host 127.0.0.1 --remote-port 4280 "done"`
