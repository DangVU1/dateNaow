CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT uuidv7(),
    email         TEXT NOT NULL CHECK (length(email) <= 254), -- lowercasing is done by Email::parse; no CHECK on lower() because Rust and Postgres may lowercase non-ASCII letters differently
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    gender        TEXT NULL CHECK (gender IN ('male', 'female', 'other', 'prefer_not_to_say')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX uq_users_email ON users (email);
