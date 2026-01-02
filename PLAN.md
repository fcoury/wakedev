# Notifall Plan (v1)

## Goals
- Build `notifall`, a Rust CLI notifier that supports multiple notification mediums.
- Start with macOS Notification Center to replace `terminal-notifier`.
- Keep the architecture extensible for future providers.

## Decisions (from user)
- Project name/crate: `notifall`.
- Config format: TOML.
- Default provider: if macOS detected, default to macOS provider.
- Must-have fields for MVP: icon, title, message (when supported by provider).

## Proposed Mediums (Future Support)
- OS native notifications: macOS, Windows Toast, Linux (libnotify/D-Bus).
- Chat apps: Telegram, Slack, Discord, Teams, Matrix.
- SMS / messaging providers: Twilio, MessageBird, Vonage, AWS SNS SMS.
- Push: APNs, FCM, Web Push (VAPID).
- Email: SMTP, SendGrid/Mailgun/SES.
- Webhook: generic HTTP POST.
- Local integrations: sound, local log/file, exit-code hooks.

## Common Notification Properties
- `title` (string)
- `message`/`body` (string)
- `icon` (path/URL; provider-specific)
- `urgency` (low/normal/high)
- `tag`/`category`
- `dedupe_key`
- `timestamp`
- `actions` (label + optional URL/command)
- `link` (URL)
- `sender`
- `metadata` (key/value)
- `dry_run`

## Implementation Plan

### Phase 1 — Core spec & CLI surface
1. Define `Notification` struct (title, message, icon, urgency, tag, link, sender, dedupe_key, metadata, actions).
2. Define TOML config model and precedence rules (CLI args override config; env optional).
3. CLI commands:
   - `notifall send --title --message [--icon] [--link] [--urgency] [--tag]`
   - `notifall config init`
   - `notifall providers list`
4. Exit codes + logging conventions.

### Phase 2 — Provider architecture
1. Provider trait: `name()` and `send()` returning `DeliveryReport`.
2. Provider registry / factory for CLI + config selection.
3. Standard error mapping across providers.

### Phase 3 — macOS provider (MVP)
1. Implement Notification Center delivery with title/message/icon/link/tag/group.
2. Match `terminal-notifier` basics: title, subtitle/body, sound, icon, open URL, group/tag.
3. Default provider if macOS detected.

### Phase 4 — Tests & docs
1. Unit tests for CLI parsing + validation.
2. Minimal docs + examples.
3. Release packaging (`cargo install`).

### Phase 5 — Roadmap Providers
1. Webhook provider.
2. Telegram + Slack.
3. SMS via Twilio.
