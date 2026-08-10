# Project Status & Task List

Status snapshot: **skeleton implemented, not yet compiled/verified.** ~4,100
lines of Rust across 33 source files + 1 migration. See AGENTS.md for the
non-negotiable architecture rules and workflow order.

## Implemented (written, NOT yet compile-verified)

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

## Not implemented / not yet done

### 1. Build & compile verification (BLOCKER)
- `cargo check` has NOT succeeded yet. Two attempts aborted: first a stuck
  `cargo metadata` process held the package-cache lock; second run was aborted
  by the user while resolving dependencies.
- Fix lock issue (`pkill cargo`, clear `~/.cargo/registry/.package-cache`
  if needed), then run `cargo check` (or `--offline` against the local cache).
- Expect a few fixups: sqlx 0.9 API deltas (e.g. `Transaction` Deref,
  feature names), enum `sqlx::Type`/`FromRow` mapping, unused-import warnings.
- After it compiles: `cargo test` (unit tests only — no DB needed) and
  `cargo clippy` + `cargo fmt`.

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

### 4. Documentation
- `README.md` does not exist: quickstart, env vars, run instructions,
  webhook setup via BotFather, local dev, deploy, security notes.
- Design deliverables 1–8 (architecture overview, component diagram, DB
  schema, domain model, conversation flows, API spec, event model, Rust
  structure) were never written as documents — only embodied in code.

### 5. Edge cases (implemented in code, NOT tested)
- invalid/duplicate session key, join already-joined, owner leaves with
  members remaining, double-complete a plan (idempotent), complete a
  cancelled plan (error), deadline past/reminders, duplicate Telegram
  updates (dedupe), blocked-bot delivery failure (recorded, non-fatal),
  session key collision (retry loop).

### 6. Deployment & security hardening
- No deployment config (Dockerfile, CI) — only `docker-compose.yml` for
  Postgres.
- No rate limiting layer on the webhook endpoint yet.
- Webhook secret-token validation and internal API auth are implemented
  but not penetration-checked.

## Remaining todo list

1. Fix cargo lock / network, then `cargo check` → fix compile errors → `cargo test` → `cargo clippy` → `cargo fmt`.
2. `docker compose up -d postgres`; run migrations; boot the server locally.
3. Integration tests (isolation + notification delivery) using `RecordingGateway`.
4. E2E test for the multi-user scenario.
5. `README.md` + design docs (architecture, schema, flows, API, events).
6. Edge-case test coverage (blocked bot, duplicate updates, owner leave, double complete).
7. Deployment: Dockerfile, CI, rate limiting, webhook lifecycle tooling.
