-- Partello initial schema.
-- Only durable data lives here: rooms, their chat history, and developer API keys.
-- Presence, playback, queue, and call state are in memory (see docs/ARCHITECTURE.md).

CREATE TABLE api_keys (
    id           UUID PRIMARY KEY,
    label        TEXT NOT NULL CHECK (char_length(label) BETWEEN 1 AND 60),
    -- SHA-256 of the key; the key itself is shown once and never stored.
    key_hash     BYTEA NOT NULL UNIQUE,
    key_prefix   TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ
);

CREATE TABLE rooms (
    id               UUID PRIMARY KEY,
    slug             TEXT NOT NULL UNIQUE CHECK (char_length(slug) = 12),
    name             TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 60),
    -- SHA-256 of the creator's host key.
    host_key_hash    BYTEA NOT NULL,
    control_mode     TEXT NOT NULL DEFAULT 'host_only' CHECK (control_mode IN ('host_only', 'everyone')),
    queue_mode       TEXT NOT NULL DEFAULT 'everyone' CHECK (queue_mode IN ('host_only', 'everyone')),
    max_participants INTEGER NOT NULL CHECK (max_participants BETWEEN 2 AND 100),
    api_key_id       UUID REFERENCES api_keys (id) ON DELETE SET NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_active_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX rooms_last_active_at_idx ON rooms (last_active_at);
CREATE INDEX rooms_api_key_id_idx ON rooms (api_key_id) WHERE api_key_id IS NOT NULL;

CREATE TABLE chat_messages (
    id             UUID PRIMARY KEY,
    room_id        UUID NOT NULL REFERENCES rooms (id) ON DELETE CASCADE,
    participant_id UUID NOT NULL,
    display_name   TEXT NOT NULL,
    body           TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX chat_messages_room_created_idx ON chat_messages (room_id, created_at DESC);
