-- Failed-attempt counters for sign-in and password-reset requests (P5).
-- `key_hash` is the SHA-256 of a label such as "login:email:an@example.com",
-- so the table never holds a readable email or IP address.
CREATE TABLE auth_attempts (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    key_hash     BYTEA NOT NULL CHECK (length(key_hash) = 32),
    attempted_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_auth_attempts_key_hash_attempted_at ON auth_attempts (key_hash, attempted_at);
