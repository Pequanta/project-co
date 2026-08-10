-- Per-member progress + session mode.
--
-- Mode selects how progress is interpreted:
--   collaboration: members split the plan list; completing a plan marks it
--                  globally done and attributes it to the completer.
--   study:         every member completes every plan independently; a member's
--                  progress is how many plans THEY have completed.
ALTER TABLE sessions ADD COLUMN mode text NOT NULL DEFAULT 'collaboration';

-- Attribution: who has completed which plan. One row per (plan, member).
--   collaboration: exactly one row per completed plan (the claimer).
--   study:         up to one row per member per plan.
CREATE TABLE plan_completions (
    plan_id      uuid NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    completed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (plan_id, user_id)
);
CREATE INDEX idx_plan_completions_user ON plan_completions (user_id);

-- Backfill attribution for any pre-existing completed plans. The historical
-- completer is unknown, so attribute to the plan's creator. No-op on an empty
-- table (fresh installs).
INSERT INTO plan_completions (plan_id, user_id, completed_at)
SELECT id, created_by, COALESCE(completed_at, now())
FROM plans
WHERE status = 'completed'
ON CONFLICT DO NOTHING;
