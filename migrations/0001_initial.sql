CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Users: Telegram users are mapped to application users (1:1 by telegram id).
CREATE TABLE users (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    telegram_user_id  bigint NOT NULL UNIQUE,
    telegram_username text,
    display_name      text NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);

-- Sessions: a collaboration space. session_key is unique + unpredictable.
CREATE TABLE sessions (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    session_key        text NOT NULL UNIQUE,
    project_name       text NOT NULL,
    project_description text,
    deadline           timestamptz NOT NULL,
    created_by         uuid NOT NULL REFERENCES users(id),
    status             text NOT NULL DEFAULT 'active', -- active | completed | archived
    last_reminder_at   timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_sessions_deadline ON sessions (deadline);
CREATE INDEX idx_sessions_status   ON sessions (status);

-- Session membership: owner | member. Enforced session isolation lives at the
-- data-access layer: every session-scoped read/write checks this table.
CREATE TABLE session_members (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       text NOT NULL DEFAULT 'member', -- owner | member
    joined_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (session_id, user_id)
);
CREATE INDEX idx_session_members_user    ON session_members (user_id);
CREATE INDEX idx_session_members_session ON session_members (session_id);

-- Plans: planned | in_progress | completed | cancelled.
CREATE TABLE plans (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id  uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    title       text NOT NULL,
    description text,
    status      text NOT NULL DEFAULT 'planned',
    created_by  uuid NOT NULL REFERENCES users(id),
    assigned_to uuid REFERENCES users(id),
    created_at  timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_plans_session       ON plans (session_id);
CREATE INDEX idx_plans_session_status ON plans (session_id, status);

-- Progress updates: append-only history, never overwritten.
CREATE TABLE progress_updates (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id),
    message    text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_progress_updates_session ON progress_updates (session_id, created_at DESC);

-- Event outbox: domain events that drive notifications.
CREATE TABLE events (
    id            bigserial PRIMARY KEY,
    event_type    text NOT NULL,
    session_id    uuid,
    actor_id      uuid,
    payload       jsonb NOT NULL DEFAULT '{}',
    created_at    timestamptz NOT NULL DEFAULT now(),
    delivered_at  timestamptz,
    delivery_error text,
    attempts      int NOT NULL DEFAULT 0
);
CREATE INDEX idx_events_undelivered ON events (delivered_at) WHERE delivered_at IS NULL;

-- Per-recipient notification attempts. Delivery failure is recorded, never fatal.
CREATE TABLE notifications (
    id                  bigserial PRIMARY KEY,
    event_id            bigint REFERENCES events(id),
    session_id          uuid REFERENCES sessions(id),
    recipient_telegram_id bigint NOT NULL,
    message             text NOT NULL,
    status              text NOT NULL DEFAULT 'pending', -- pending | sent | failed
    error               text,
    attempts            int NOT NULL DEFAULT 0,
    created_at          timestamptz NOT NULL DEFAULT now(),
    sent_at             timestamptz
);
CREATE INDEX idx_notifications_pending ON notifications (status) WHERE status = 'pending';

-- Idempotency: dedupe duplicate Telegram updates (same update_id).
CREATE TABLE processed_updates (
    update_id    bigint PRIMARY KEY,
    processed_at timestamptz NOT NULL DEFAULT now()
);

-- Serverless bot conversation state (state machine + temporary payload).
CREATE TABLE bot_conversations (
    user_id    uuid PRIMARY KEY REFERENCES users(id),
    chat_id    bigint NOT NULL,
    state      text NOT NULL DEFAULT 'idle',
    payload    jsonb NOT NULL DEFAULT '{}',
    updated_at timestamptz NOT NULL DEFAULT now()
);
