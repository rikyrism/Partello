-- The current video and queue of a room, saved so restarts and idle unloads
-- do not lose them (JSON written by the server, see room::state::SavedState).
ALTER TABLE rooms ADD COLUMN saved_state JSONB;
-- Permanent rooms are never purged for inactivity, so their link keeps working.
ALTER TABLE rooms ADD COLUMN permanent BOOLEAN NOT NULL DEFAULT false;
-- The quoted message a chat reply answers ({id, participant_id, display_name, body}).
ALTER TABLE chat_messages ADD COLUMN reply_to JSONB;
