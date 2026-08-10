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

The bot supports `/start`, `/create`, `/join`, `/status`, `/progress`, `/plan`,
`/plans`, `/complete`, `/members`, `/leave`, and `/help`. Inline buttons are
used whenever a session or plan needs selecting.

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
