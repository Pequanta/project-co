# Project Co design

## Architecture

PostgreSQL is authoritative. Telegram is an inbound interaction channel and
outbound notification transport only. The executable is an Axum webhook
service; it does not poll Telegram.

```text
Telegram -> /webhook -> Bot router -> application services -> repositories -> PostgreSQL
                                     |                              |
                                     +-> domain-event outbox --------+-> notification service -> Telegram
Internal scheduler/API ------------------------------------------------^
```

The adapter depends on application services, never the reverse. Repository
traits make persistence replaceable, and `TelegramGateway` makes the outbound
Bot API transport replaceable and testable.

## Database and domain model

The migration creates `users`, `sessions`, `session_members`, `plans`,
`progress_updates`, `events`, `notifications`, `processed_updates`, and
`bot_conversations`.

`session_members(session_id, user_id)` is unique. Every session-scoped service
operation checks that membership before reading or mutating session data.
Plans use `planned`, `in_progress`, `completed`, or `cancelled`; progress
updates are append-only. A session key is a collision-checked, cryptographically
random eight-character key, normalized before lookup.

Progress is deterministic: `completed / (planned + in_progress + completed) *
100`; cancelled plans are excluded. It is rendered with a ten-cell bar.

## Telegram flows

`/start` registers (or refreshes) the Telegram user and shows the main menu.
`/create` collects name, optional description, deadline, then optional initial
plans. `/join` collects a formatted session key. `/status`, `/plans`,
`/members`, `/progress`, `/plan`, `/complete`, and `/leave` select a session
with inline buttons when necessary. Conversation state is persisted in
`bot_conversations`, so webhook invocations remain stateless.

All command and callback paths resolve a selected session through the user's
membership list; service-level authorization is the definitive check.

## HTTP API and events

`POST /webhook` accepts Telegram updates only when its secret-token header
matches `BOT_WEBHOOK_SECRET`. It deduplicates `update_id` in PostgreSQL.
`GET /healthz` returns 200 when the process is alive. `POST
/internal/notifications` accepts a deadline event only with `Authorization:
Bearer <INTERNAL_API_KEY>`.

Domain events include session creation/joining, plan lifecycle, progress
updates, and deadline reminders. They are written to the `events` outbox before
delivery. Notification attempts are recorded per recipient; a Telegram delivery
failure is logged and recorded but never rolls back a business operation.

## Scaling notes

Run multiple stateless webhook instances behind TLS termination. Keep a shared
PostgreSQL database so deduplication and conversations remain correct. For
larger delivery volume, move pending-event dispatch into a separate worker and
use row locking/leases. Replace the in-process inbound limiter with a shared
Redis or gateway limiter when instances are horizontally scaled.
