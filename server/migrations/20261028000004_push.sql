-- Small server-wide values, e.g. the VAPID key pair push subscriptions are tied to.
CREATE TABLE server_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Browsers that asked to be notified when someone starts watching in a room.
-- The endpoint is a capability URL at a browser push service; keys encrypt the payload.
CREATE TABLE push_subscriptions (
    id         UUID PRIMARY KEY,
    room_id    UUID NOT NULL REFERENCES rooms (id) ON DELETE CASCADE,
    endpoint   TEXT NOT NULL CHECK (char_length(endpoint) <= 1024),
    p256dh     TEXT NOT NULL,
    auth       TEXT NOT NULL,
    lang       TEXT NOT NULL DEFAULT 'en' CHECK (lang IN ('en', 'id')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (room_id, endpoint)
);
