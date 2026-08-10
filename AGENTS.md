# AGENTS.md

## Project status

This repository is **empty** — no code, toolchain, or git repo exists yet. The
product spec below is the agreed source of truth. Do not assume any files,
dependencies, or setup exist until they are actually scaffolded.

## What this is

A production-grade **Telegram collaboration & progress tracking system** in Rust.
One-to-one or small-team collaboration sessions; users join via a unique session
key, submit progress updates, manage plans, and get notifications. Telegram is
the bot UI; a backend service triggers activities/notifications through the bot.

Reference: https://core.telegram.org/bots/serverless

## Non-negotiable architecture rules

These are the highest-risk points an agent will get wrong:

- **Backend/database is the source of truth**, never Telegram. Telegram is only
  the interaction and notification interface. Do not couple business logic to
  Telegram API calls.
- **Telegram Serverless Bots**, not a long-running polling process. Bot entry is
  via serverless/webhook updates; validate webhook requests per Telegram's
  recommended mechanism and dedupe updates.
- **Session isolation is enforced at the data-access/authorization layer**, not
  only in bot logic. A user must only ever receive/see data from sessions they
  belong to; validate every session operation against membership server-side.
- Layered architecture with clear traits/interfaces so the Telegram adapter is
  replaceable without touching business logic:
  `Telegram Adapter → Application Services → Repositories → Notification Service → Event System`.
- Business events (e.g. `PlanCompleted`, `ProgressUpdated`) drive the
  notification subsystem. Delivery failure (blocked bot, backend notification
  error) must be handled, not fatal.
- Progress calculation is deterministic and pluggable:
  `progress = completed_plans / total_active_plans × 100` (cancelled excluded).
  Display as percentage plus visual bar (`██████░░░░`).

## Workflow convention

Per spec, do **not** jump straight into code. Produce deliverables in this order
and get design approved before implementing:

1. Architecture overview → 2. component diagram → 3. database schema → 4. domain
   model → 5. Telegram conversation flows → 6. API spec → 7. event model →
   8. Rust project structure → 9. implementation → 10. migrations →
   11. env config → 12. tests → 13. local dev → 14. deployment →
   15. security → 16. future scalability.

Surface assumptions/ambiguities up front.

## Data model

- Entities: `User`, `Session` (unique, cryptographically secure `session_key`,
  deadline, status), `SessionMember` (roles: `owner`/`member`), `Plan`
  (statuses: `planned`/`in_progress`/`completed`/`cancelled`), `ProgressUpdate`
  (append-only history — never overwrite prior updates), plus `events` /
  `notifications`.
- Persistence: **PostgreSQL** with migrations for
  `users, sessions, session_members, plans, progress_updates, events,
  notifications`; FKs + indexes on telegram user id, session key, membership,
  per-session plans/progress updates, and deadline.
- Use DB transactions for: create/join session, create/complete plan, submit
  progress update, broadcast notification.

## Telegram UX

- Commands: `/start /create /join /status /progress /plan /plans /complete
  /members /help /leave`. Prefer inline keyboards/buttons over memorized
  commands; never force users to type raw IDs.
- `/start` must work for users who never registered (guide registration).

## Security

- Cryptographically secure, unguessable session keys (handle collisions).
- Protect bot token and any internal API (internal `/notifications` endpoint
  must be authenticated; no arbitrary users may trigger it).
- Idempotency for important operations; rate limiting; sanitize user content;
  audit trail for state changes.

## Testing

- Unit: progress calc, session-key generation, authorization, plan state
  transitions, deadline calc, event generation.
- Integration: registration, join, plan lifecycle, progress updates, session
  isolation, notification delivery.
- E2E at minimum: A creates session → B joins → A creates plan → B submits
  progress → A receives it → B completes plan → both see updated progress → C
  (in another session) receives nothing.

## Rust conventions

Idiomatic, typed Rust; async I/O; clear module boundaries; error handling;
structured logging; config via environment variables; no unnecessary
abstractions.
