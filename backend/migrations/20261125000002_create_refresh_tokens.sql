-- Refresh tokens (P3). Only the SHA-256 hash of the token is stored.
-- `replaced_at` is set only by rotation; any other revocation (logout,
-- password reset, reuse detection) sets `revoked_at` alone.
CREATE TABLE refresh_tokens (
    id          UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  BYTEA NOT NULL CHECK (length(token_hash) = 32),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ NULL,
    replaced_at TIMESTAMPTZ NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX uq_refresh_tokens_token_hash ON refresh_tokens (token_hash);
CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens (user_id);
