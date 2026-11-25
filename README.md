# dateNaow

Mobile-first web app for couples in Saigon: where to go on a date and what to gift.
Backend: Rust (Axum, SQLx, PostgreSQL 18). Frontend: React + Vite + TypeScript.
Product spec: `docs/superpowers/specs/2026-11-23-datenaow-design.md`.
Engineering conventions and pinned versions: `.claude/CLAUDE.md`.

## Prerequisites

- Docker with Docker Compose v2
- Node.js 24 and npm 11 (see `frontend/.nvmrc`)
- Rust stable ≥ 1.94 and `sqlx-cli` 0.9.1, only for running the backend outside Docker or regenerating `backend/.sqlx`

## Run locally

```sh
cp backend/.env.example backend/.env        # dev-only values; never commit .env
docker compose up --build -d                # backend :8080, PostgreSQL :5432, Mailpit :8025
cd frontend
npm ci
npm run dev                                 # http://localhost:5173, proxies /api to :8080
```

- Password-reset emails arrive in Mailpit: http://localhost:8025
- API description: http://127.0.0.1:8080/api/v1/openapi.json
- Make a user an admin (PostgreSQL must be up; run from `backend/`):
  `cargo run --bin make_admin -- <email>`, then sign out and in again.

Docker Compose is for local development only. Behind its published port every client
appears to the backend with the same IP, so the per-IP limits are shared and the
per-(email, IP) limits act like per-email limits (one person can lock out another); a real
deployment needs a reverse proxy that appends the client address and `TRUST_PROXY=true`.

To run the backend with `cargo run` instead of Docker, start only the services it needs
(`docker compose up -d postgres mailpit`), then `cd backend && cargo run`.

## Checks

Backend, from `backend/` (PostgreSQL must be running for the tests):

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo sqlx prepare --check -- --all-targets   # offline query data used by the Docker build
```

Frontend, from `frontend/`:

```sh
npm run lint
npm run format:check
npm run typecheck
npm test
npm run build
```

## Third-party data

The backend embeds the SecLists `10k-most-common.txt` password list (MIT licence,
Copyright (c) 2018 Daniel Miessler) to reject common passwords; see
`backend/src/modules/auth/domain/value_objects/common_passwords/SECLISTS_LICENSE`.
