# Project Status & Task List

Status snapshot: **core implementation is compile-verified and locally
database-verified.** See AGENTS.md for the non-negotiable architecture rules
and workflow order.

## Implemented and verified

| Area | Files | Notes |
| --- | --- | --- |
| Scaffolding | `Cargo.toml`, `.gitignore`, `.env.example`, `docker-compose.yml` | sqlx 0.9, axum 0.8, tokio, reqwest; pinned to crates present in local cargo cache |
| Database schema | `migrations/0001_initial.sql` | users, sessions, session_members, plans, progress_updates, events (outbox), notifications, processed_updates (dedupe), bot_conversations |
| Domain | `src/domain/` | models + enums, events, progress calc (pluggable), crypto session keys, errors. Unit tests written for progress calc / keys / plan transitions |
| Repositories | `src/repo/` | traits (replaceable) + Postgres impls; `&mut PgConnection` so transactions compose |
| App services | `src/app/` | sessions, plans, progress, users + `authorization::assert_member` (session isolation at data-access layer) |
| Event system | `src/eventing.rs` | outbox persist → dispatch; delivery failures non-fatal |
| Notifications | `src/notify/` | broadcast to members minus actor; per-recipient `notifications` rows, failures recorded |
| Telegram adapter | `src/telegram/` | typed Bot API client, webhook handler (secret-token validate + update_id dedupe), router + conversation state machine, inline keyboards |
| Internal API | `src/http/` | `POST /internal/notifications` (Bearer auth), `GET /healthz` |
| Runtime | `src/main.rs`, `config.rs`, `state.rs`, `jobs.rs`, `telemetry.rs` | wiring, env config, deadline-reminder background sweep, tracing |
| Security | `src/rate_limit.rs` | per-source-IP fixed-window webhook limiter; configurable by environment |
| Operations | `Dockerfile`, `render.yaml`, `.github/workflows/ci.yml`, `README.md`, `docs/DESIGN.md` | non-root image, Render Blueprint, CI verification, quickstart and design/operations record |

## Not implemented / not yet done

### 1. Build & compile verification — complete
- `cargo fmt --check`, `cargo check --offline`, `cargo test --offline` (13
  tests), and `cargo clippy --offline -- -D warnings` pass.
- A clean temporary PostgreSQL instance was used to verify migration/startup;
  the application created all expected public schema tables.

### 2. Integration & E2E tests (needs a database)
- No integration tests exist yet. Spec requires: registration, join, plan
  lifecycle, progress updates, **session isolation**, notification delivery.
- E2E minimum: A creates session → B joins → A creates plan → B submits
  progress → A receives it → B completes plan → both see updated progress →
  C (other session) receives nothing.
- `RecordingGateway` test double already exists in
  `src/telegram/gateway.rs` (`test_gateway` module) — not yet wired into tests.

### 3. Local dev run-through (needs a database)
- `docker compose up -d postgres` has not been started or verified.
- Startup flow not exercised: migrations apply, webhook setWebhook
  (only when `BOT_WEBHOOK_URL` set), reminder job spawn.
- No real-bot smoke test: commands, create/join flow, progress, complete,
  status dashboard, inline keyboards.

### 4. Documentation — complete
- `README.md` covers quickstart, configuration, verification, deployment and
  security. `docs/DESIGN.md` records architecture, schema/domain model,
  conversation flows, API/event model, and scalability.

### 5. Edge cases (implemented in code, NOT tested)
- invalid/duplicate session key, join already-joined, owner leaves with
  members remaining, double-complete a plan (idempotent), complete a
  cancelled plan (error), deadline past/reminders, duplicate Telegram
  updates (dedupe), blocked-bot delivery failure (recorded, non-fatal),
  session key collision (retry loop).

### 6. Deployment & security hardening
- Dockerfile, CI, webhook secret validation, internal API bearer auth, and an
  in-process per-IP webhook limiter are implemented.
- A shared rate limiter (for example at the gateway or Redis) is recommended
  before horizontally scaling beyond one instance.

## Remaining todo list

1. Add automated PostgreSQL integration tests for isolation and notification
   delivery using `RecordingGateway`.
2. Add the full multi-user E2E scenario and edge-case coverage (blocked bot,
   duplicate updates, owner leave, and double completion).
3. Before shared, multi-instance deployment, move rate limiting to a shared
   gateway/store and add worker-based outbox retry processing.
