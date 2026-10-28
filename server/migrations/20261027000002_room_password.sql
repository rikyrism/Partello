-- Optional room password, stored as an Argon2id PHC string (never plaintext).
ALTER TABLE rooms ADD COLUMN password_hash TEXT;
