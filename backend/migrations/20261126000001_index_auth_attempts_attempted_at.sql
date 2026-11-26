-- Serves the prune query of the attempt limiter (`DELETE FROM auth_attempts
-- WHERE attempted_at < $1`, run on every acquire), which the
-- (key_hash, attempted_at) index cannot use (DECISIONS #17).
CREATE INDEX idx_auth_attempts_attempted_at ON auth_attempts (attempted_at);
