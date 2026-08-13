# Project Co

Telegram-based small-team collaboration and progress tracking, built in Rust.
PostgreSQL is the source of truth; Telegram is a webhook UI and notification
channel.

## Run locally

1. Copy `.env.example` to `.env` and replace every example secret. Generate
   long values for `BOT_WEBHOOK_SECRET` and `INTERNAL_API_KEY` (for example,
   `openssl rand -hex 32`).
2. Start PostgreSQL: `docker compose up -d postgres`.
3. Export the variables from `.env` using your shell's safe env-file mechanism,
   then run `cargo run`. Startup applies migrations automatically.
4. Expose `http://localhost:8080/webhook` through an HTTPS tunnel for local bot
   testing, set `BOT_WEBHOOK_URL` to that public URL, and restart the process.

The bot supports `/start`, `/create`, `/join`, `/sessions`, `/status`,
`/progress`, `/plan`, `/plans`, `/complete`, `/members`, `/leave`, and `/help`.
Use `/sessions` (or **My sessions**) to open an existing session. Inline buttons
are used whenever a session or plan needs selecting; users never need to type a
database session UUID. To join a session they do not belong to, they enter its
shareable session key through `/join`.

## Broadcast to a group or channel

By default each session's updates are delivered as private DMs to its members.
A session can instead post its updates into a Telegram group or channel:

1. Add the bot to the group, or add it as an **admin** of the channel (a channel
   admin is required for the bot to receive posts).
2. In that chat, send `/link <SESSION_KEY>` — the same key used to `/join`.
3. From then on, that session's notifications post in the chat and member DMs
   are skipped. Send `/unlink` to stop (or `/unlink <SESSION_KEY>` to remove
   just one session when several are linked to the chat).

Anyone holding the session key can link a chat they control, which is the same
capability `/join` already grants. Interactive commands (create, join, progress,
etc.) remain private-DM only; groups and channels understand only `/link` and
`/unlink`.

## Configuration

Required: `DATABASE_URL`, `BOT_TOKEN`, `BOT_WEBHOOK_SECRET`, and
`INTERNAL_API_KEY`. Optional: `BOT_WEBHOOK_URL`, `HTTP_ADDR`,
`DEADLINE_REMINDER_WINDOW_HOURS`, `DEADLINE_REMINDER_INTERVAL_HOURS`, and
`WEBHOOK_RATE_LIMIT_PER_MINUTE`. See `.env.example` for meanings and defaults.

The webhook secret must be configured through Telegram's `setWebhook` request;
the app does this on startup if `BOT_WEBHOOK_URL` is supplied.

## Verification

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked -- -D warnings
```

The production image is built with `docker build -t project-co .`. Supply all
required configuration at runtime and place the service behind an HTTPS reverse
proxy. The image runs as a non-root user.

## Deploy on Render

Push this repository to GitHub or GitLab, then choose **New → Blueprint** in
Render and select the repository. The included `render.yaml` creates the
`project-co` Docker web service and a colocated Render Postgres database.

Render prompts for `BOT_TOKEN`, `BOT_WEBHOOK_SECRET`, and `INTERNAL_API_KEY`;
provide long, distinct secret values. Do not add them to `render.yaml`. The app
binds to Render's `PORT`, runs migrations at startup, and uses
`RENDER_EXTERNAL_URL` to register `https://<service>.onrender.com/webhook`
with Telegram automatically. Configure a custom domain before deployment if
you want Telegram to use that instead.

The Blueprint defaults to Render's free plans for evaluation. Free services
can spin down and free Postgres expires after 30 days, so use paid plans and
backups for a production bot. After the deploy is healthy, send `/start` to
the bot and confirm `https://<service>.onrender.com/healthz` returns 200.

## Security and operations

Never commit `.env` or a bot token. Webhook requests require Telegram's secret
header, are deduplicated, and are rate limited per TCP source address. The
internal notifications endpoint is bearer-authenticated. Database membership
checks protect session isolation independently of the Telegram router. Content
is trimmed and bounded by service validation, and progress history is
append-only for auditing.

Before production, provision managed PostgreSQL backups, set restrictive
network access, use a secrets manager, configure TLS, and alert on failed
notification rows and migration/startup errors. See [the design](docs/DESIGN.md)
for architecture, event, API, and scaling details.
