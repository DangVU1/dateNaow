# syntax=docker/dockerfile:1
# Multi-stage build for the dateNaow backend. Build from backend/:
#   docker build -t datenaow-backend .
# Queries are checked against the committed .sqlx/ data (SQLX_OFFLINE), so the
# build needs no database. Migrations are embedded by `sqlx::migrate!()`.

FROM rust:1.100.0-slim-trixie AS build
WORKDIR /app
ENV SQLX_OFFLINE=true

# Build the dependencies first against stub sources, so this layer is reused
# until Cargo.toml or Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && touch src/lib.rs \
    && cargo build --release --locked --bin datenaow \
    && rm -rf src

COPY .sqlx ./.sqlx
COPY migrations ./migrations
COPY src ./src
# Only the server binary is built; run make_admin from the host with
# `cargo run --bin make_admin -- <email>` against 127.0.0.1:5432.
# COPY keeps the source mtimes, which can be older than the stubs above.
RUN touch src/main.rs src/lib.rs \
    && cargo build --release --locked --bin datenaow

FROM debian:trixie-20261123-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin datenaow
COPY --from=build /app/target/release/datenaow /usr/local/bin/datenaow
USER datenaow
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
CMD ["datenaow"]
