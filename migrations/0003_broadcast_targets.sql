-- Broadcast targets: Telegram groups/channels a session posts notifications to.
--
-- When a session has one or more targets, notifications are delivered to those
-- chats instead of member DMs. A chat is linked by posting `/link <SESSION_KEY>`
-- inside it, so the secret session key is the linking capability (same trust
-- model as joining). Channel posts carry no user, so `linked_by` is nullable.
CREATE TABLE session_broadcast_targets (
    session_id uuid   NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    chat_id    bigint NOT NULL,
    chat_type  text   NOT NULL,               -- group | supergroup | channel
    title      text,
    linked_by  uuid   REFERENCES users(id),   -- NULL for channel posts
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, chat_id)
);
CREATE INDEX idx_broadcast_targets_session ON session_broadcast_targets (session_id);
CREATE INDEX idx_broadcast_targets_chat    ON session_broadcast_targets (chat_id);
