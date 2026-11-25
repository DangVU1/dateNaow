# dateNaow — Project Instructions

## 0. Project Context

dateNaow is a mobile-first web app for couples (dating or newly met) in Saigon (TP.HCM). It answers two questions — "where do we go?" (date places/activities) and "what do I gift?" — delivered through a small pixel cat that peeks in from the screen edge with a "Bạn có biết?" speech bubble.

### 0.1 Sources of truth

| Topic | Source |
|---|---|
| Product requirements, scope, recommendation rules | `docs/superpowers/specs/2026-11-23-datenaow-design.md` |
| Stack, architecture, engineering conventions | This file |
| Decision log (date, decision, rationale, superseded entries) | `docs/DECISIONS.md` |
| Cross-project working rules (approval, reporting, language) | Global `~/.claude/CLAUDE.md` |

Rules for reading the spec:
- The spec was **approved in full on 2026-11-25**. Every item in it is a decision; changing one needs the owner's approval and an entry in `docs/DECISIONS.md`.
- Items listed in spec §9 ("Để lại cho người xây dựng") are left to the implementer: propose a choice, then record it in `docs/DECISIONS.md` (and here or in the spec, where it belongs) once approved.

### 0.2 Current state

- No application code yet. The repository contains only `.claude/` and `docs/`.
- Commits so far: the initial `.claude/` + draft spec, then the approved spec, decision log and implementation plans (2026-11-25). Nothing pushed.
- Stack versions are not pinned yet (see §2).
- Implementation is split into four plans (`docs/DECISIONS.md` #5). Roadmap: `docs/superpowers/plans/2026-11-25-datenaow-roadmap.md`. Plan 1 (foundation + accounts) is approved (`docs/DECISIONS.md` #9) and will be executed subagent-driven (#10). Plan file: `docs/superpowers/plans/2026-11-25-plan-1-foundation-accounts.md`.

Update this section when the state changes materially (scaffolding done, first release, etc.).

### 0.3 Product invariants (must never be violated)

These come from spec §2 ("Điều phải tránh") and §8. Breaking any of them is a failure even if the code otherwise works. Enforce them in the **Application layer**, not only in middleware, and cover each with tests.

- **"Về người ấy" (partner notes)** are visible only to their author — never to the partner, even after linking. No API path may return them to anyone else.
- **Wishes ("Người kia muốn gì")** are visible only to their author. The partner only receives a pre-selected gift suggestion; the raw wish text is never quoted to the giver.
- The recipient must never learn which of their wishes were suggested. "Already suggested" history is internal only.
- **Surprise date plans** are visible only to their creator.
- **Gift budgets** belong to each person and are never shown to the partner.
- **Admin APIs** require the `admin` role.
- The **unrecognised-keyword list** must never expose users' private sentences to admins (spec §7.2).
- Location is used only for place recommendation.
- UI: the cat and its speech bubble stay small and in one corner; adding a wish is a single free-text line, never a long form.

### 0.4 Out of scope for v1

Per spec §2: shop recommendations, free chat between users, push notifications and reminder emails (email only for password reset), Google/OTP login, AI/LLM usage, user-submitted places, cities other than Saigon, native mobile apps, payments. Do not build these without explicit approval.

---

## 1. Role & Responsibilities

Act as a Senior Rust Backend Engineer and Software Architect. When working on the frontend, act as a senior React/TypeScript engineer following the same priorities.

Follow the architecture, coding conventions, and engineering standards defined in this document. Sections 3–17 apply to the backend; frontend conventions are in §18.

Priorities:
1. Correctness and reliability.
2. Maintainable and scalable architecture.
3. Type safety and memory safety.
4. Clear separation of responsibilities.
5. Performance and resource efficiency.
6. Security and data integrity.
7. Testability.
8. Avoid unnecessary complexity.

Always inspect the existing codebase before making architectural decisions.

Do not blindly apply patterns or introduce abstractions without understanding their purpose.

---

## 2. Approved Technology Stack

The stack below is the project owner's decision (spec §3). Do not replace any component without impact analysis and explicit approval.

### 2.1 Backend

| Component | Technology |
|---|---|
| Language | Rust (stable) |
| Web Framework | Axum |
| Async Runtime | Tokio |
| Architecture | Modular Monolith |
| Architecture Pattern | Clean Architecture |
| Application Pattern | CQRS when appropriate |
| Database | PostgreSQL |
| Database Toolkit | SQLx |
| Serialization | Serde |
| Validation | validator |
| Authentication | JWT |
| Password Hashing | Argon2 |
| Error Handling | thiserror |
| Error Context | anyhow (application boundaries only) |
| Logging | tracing |
| API Documentation | Utoipa |
| Database Migration | SQLx Migrate |
| Testing | Built-in Rust Test Framework |
| Containerization | Docker |
| Realtime | Axum built-in WebSocket (date-plan place-picking chat only) |

### 2.2 Frontend

| Component | Technology |
|---|---|
| Framework | React |
| Build Tool | Vite |
| Language | TypeScript |
| Target | Mobile-first web app; admin area at `/admin` in the same app |
| Router | React Router v8, library (declarative) mode |
| Server state | TanStack Query v5 |
| Client state | React Context + `useReducer` (no global store library) |
| Folder structure | Feature-based: `src/app/`, `src/features/<feature>/`, `src/shared/` |
| Lint / format | ESLint (flat config, typescript-eslint, react-hooks rules) + Prettier |
| Testing | Vitest + React Testing Library + jsdom; Playwright e2e from plan 3 |

Approved 2026-11-25 (`docs/DECISIONS.md` #6).

### 2.3 Infrastructure

| Component | Technology |
|---|---|
| Local environment | Docker Compose: backend, PostgreSQL 18, Mailpit (dev email) |

Extra backend crates approved on 2026-11-25 (`docs/DECISIONS.md` #7): tower-http, tracing-subscriber, axum-extra (cookies), uuid, dotenvy, chrono. Failed-login limits use a PostgreSQL counter table, not a rate-limiting crate.

### 2.4 Versions

Backend versions verified on 2026-11-25 (plan 1, Task 1) with `cargo search` / `cargo info`, and Docker tags with `docker manifest inspect`. Crates are pinned exactly (`=x.y.z`) in `backend/Cargo.toml`; `Cargo.lock` is committed. A crate marked "later" is not in `Cargo.toml` yet: the task that first uses it adds it at this version (`cargo add <crate>@=<x.y.z>`).

| Component | Version | Features | In Cargo.toml |
|---|---|---|---|
| Rust toolchain | 1.100.0 stable (`rust-version = "1.94"`, required by sqlx 0.9) | | |
| axum | 0.8.9 | default | yes |
| tokio | 1.54.1 | macros, rt-multi-thread, net, signal, time | yes |
| tower | 0.5.3 | timeout, util | yes |
| tower-http | 0.7.1 (uses `http` 1 and `tower` 0.5, same as axum 0.8.9; no duplicates in `cargo tree -d`) | trace, request-id | yes |
| sqlx | 0.9.1 (no default features) | runtime-tokio, postgres, uuid, chrono, migrate, macros | yes |
| serde / serde_json | 1.0.229 / 1.0.154 | serde: derive | yes |
| anyhow | 1.0.105 | | yes |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | subscriber: env-filter | yes |
| chrono | 0.4.45 | serde | yes |
| dotenvy | 0.15.7 | | yes |
| http-body-util (dev only) | 0.1.5 | | yes |
| axum-extra | 0.12.6 (the axum 0.8 line) | cookie | later |
| jsonwebtoken | 12.0.0 (no default features: PEM keys are not needed for HS256) | rust_crypto (pure-Rust RustCrypto backend; builds on Windows MSVC without the C/CMake toolchain `aws_lc_rs` needs) | yes |
| argon2 | 0.6.0 | default (alloc, getrandom, password-hash; salts come from the OS RNG via `getrandom`, so no `rand` here) | yes |
| validator | 0.21.0 (its derive resolves to `validator_derive` 0.20.1) | derive | yes |
| utoipa | 6.0.0 | default (macros) now; chrono, uuid added by the task that first needs them | yes |
| lettre | 0.11.24 | tokio1, smtp-transport, builder | later |
| thiserror | 2.0.21 | | yes |
| uuid | 1.29.3 | serde (ids are generated by PostgreSQL `uuidv7()`, so no `v7` feature) | yes |
| rand | 0.10.4 (no default features; already in the tree via sqlx-postgres) | sys_rng (tokens read `rand::rngs::SysRng`, the OS RNG, directly) | yes |
| sha2 | 0.11.1 (no default features; same version sqlx already uses) | | yes |
| sqlx-cli (global tool) | 0.9.1 | | installed globally |
| PostgreSQL image | `postgres:18.7` (latest 18.x tag) | | |
| Mailpit image | `axllent/mailpit:v1.34.0` | | |

Frontend versions are verified and recorded here in plan 1, Task 11.

If a new major version's API differs from the plan and cannot be fixed within the task, pin the previous stable major instead and record the reason here (plan 1, "Phiên bản và phụ thuộc").

Avoid unnecessary dependencies. Adding a new crate or npm package requires approval (global rules).

---

## 3. Architectural Principles

### 3.1 Modular Monolith

Organize the application into independent business modules.

Each module must:
- Represent a specific business domain.
- Encapsulate its own business rules.
- Own its application logic.
- Expose functionality through controlled interfaces.
- Avoid accessing another module's internal implementation directly.

Modules (spec §8, approved 2026-11-25):

| Module | Responsibility |
|---|---|
| `auth` | sign-up, sign-in, JWT, refresh tokens, password reset |
| `users` | user profile, roles (`user`, `admin`) |
| `couples` | invite codes, linking, unlinking, anniversary date |
| `partner_notes` | "Về người ấy" notes |
| `wishes` | wish list |
| `text_analysis` | keyword dictionary, free-text analyzer (spec §7.2), unrecognised-keyword list |
| `places` | curated place catalogue, place recommendation (spec §7.1) |
| `date_plans` | date plans, place-picking chat, pending pick, WebSocket |
| `gifts` | gift-idea catalogue, gift occasions (§7.3), gift selection (§7.4), suggestion history |
| `admin` | admin routes and permission checks only; business logic stays in the modules above |

The free-text analyzer (`text_analysis`) and the scoring rules for places and gifts are pure domain logic: keep them free of infrastructure dependencies so they are unit-testable.

Do not split into microservices by default.

### 3.2 Clean Architecture

Use four logical layers.

**Domain**
- Entities
- Value Objects
- Domain Enums
- Repository Traits
- Domain Services
- Business Rules

**Application**
- Use Cases
- Command Handlers
- Query Handlers
- Application Services
- Workflow Orchestration
- Application Interfaces

**Infrastructure**
- SQLx Repository Implementations
- Database Access
- External API Integrations
- Persistence Implementations

**Presentation**
- Axum Routes
- HTTP Handlers
- Request DTOs
- Response DTOs
- HTTP Extractors

Dependency direction:

Presentation → Application → Domain

Infrastructure implements interfaces defined by inner layers.

The Domain layer must not depend on Axum, SQLx, HTTP, or external infrastructure.

Use Rust traits for dependency inversion when they provide meaningful value.

Do not create traits for every struct automatically.

### 3.3 Dependency Injection

Rust does not require a NestJS-style dependency injection container.

Prefer:
- Constructor-based dependency injection.
- Explicit application state.
- Shared dependencies through Arc when necessary.
- Trait objects or generics only when justified.

Use Axum State for application dependencies.

Avoid global mutable state and unnecessary service locators.

---

## 4. Standard Project Structure

Repository root layout (approved 2026-11-25):

```text
dateNaow/
├── backend/            # Rust/Axum service (structure below)
├── frontend/           # React + Vite + TypeScript app
├── docs/               # specs, decisions
└── docker-compose.yml  # backend, PostgreSQL, Mailpit
```

Backend structure, used as the default starting point:

```text
backend/
├── Cargo.toml
├── Cargo.lock
├── .env.example
├── .gitignore
├── Dockerfile
│
├── migrations/
├── docs/
│
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── routes.rs
│   │
│   ├── config/
│   ├── infrastructure/
│   ├── shared/
│   │
│   └── modules/
│       ├── mod.rs
│       ├── auth/
│       ├── users/
│       ├── couples/
│       ├── partner_notes/
│       └── ...         # see §3.1
│
└── tests/
    ├── common/
    ├── integration/
    └── fixtures/
```

### 4.1 Module Structure

Each business module should follow:

```text
users/
├── mod.rs
│
├── domain/
│   ├── mod.rs
│   ├── entities/
│   ├── value_objects/
│   ├── enums/
│   ├── repositories/
│   └── services/
│
├── application/
│   ├── mod.rs
│   ├── create_user/
│   │   ├── mod.rs
│   │   ├── command.rs
│   │   └── handler.rs
│   │
│   ├── update_user/
│   │   ├── mod.rs
│   │   ├── command.rs
│   │   └── handler.rs
│   │
│   ├── get_user_by_id/
│   │   ├── mod.rs
│   │   ├── query.rs
│   │   └── handler.rs
│   │
│   └── services/
│
├── infrastructure/
│   ├── mod.rs
│   └── repositories/
│       ├── mod.rs
│       └── user_repository.rs
│
└── presentation/
    ├── mod.rs
    ├── routes.rs
    ├── handlers/
    └── dto/
```

Important conventions:

- Do not create separate commands/ and queries/ directories.
- Each use case has its own directory.
- Command and Query definitions stay inside their respective use case.
- Keep HTTP handlers in Presentation.
- Keep business workflows in Application.
- Keep database implementations in Infrastructure.
- Keep domain rules in Domain.
- Do not mix SQLx models with Domain Entities.
- Do not create empty folders.
- Small modules may use simpler structures.
- Add layers only when justified by business complexity.

### 4.2 Test Organization

Keep tests outside production source code whenever practical.

Recommended:

```text
users/
├── domain/
├── application/
├── infrastructure/
├── presentation/
└── tests/
    ├── application/
    ├── domain/
    ├── infrastructure/
    └── presentation/
```

Use Rust's built-in unit testing conventions when tests are tightly coupled to private implementation details.

Use integration tests in the root tests/ directory for public module behavior and API workflows.

Do not force all tests into separate files if colocated unit tests are significantly more appropriate.

---

## 5. CQRS & Use Case Organization

Use CQRS when read and write operations have meaningfully different responsibilities.

### Commands

Commands modify application state.

Examples:
- CreateUserCommand
- UpdateUserCommand
- DeleteUserCommand
- FavoriteActivityCommand

### Queries

Queries retrieve data without modifying application state.

Examples:
- GetUserByIdQuery
- GetUserListQuery
- GetActivityDetailsQuery

### Naming

| Component | Convention |
|---|---|
| Command | CreateUserCommand |
| Query | GetUserByIdQuery |
| Handler | CreateUserHandler |
| Use Case | CreateUser |
| HTTP Handler | create_user |
| Repository Trait | UserRepository |
| Repository Implementation | PgUserRepository |

Rust does not require a CQRS framework.

Implement CQRS using normal structs, traits, and functions.

Use cases should have explicit inputs and outputs.

Do not introduce message buses or event buses unless the project genuinely requires them.

For simple CRUD, conventional application services are acceptable.

---

## 6. Axum API Standards

Controllers in traditional frameworks are represented by Axum routes and HTTP handlers.

Responsibilities:
- Define API routes.
- Extract request data.
- Validate input.
- Delegate to Application.
- Convert application results into HTTP responses.

HTTP handlers must remain thin.

Example:

```rust
pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let result = state
        .create_user
        .execute(payload.into())
        .await?;

    Ok(Json(result.into()))
}
```

Use:
- Router for route organization.
- State for shared application dependencies.
- Extractors for typed request data.
- IntoResponse for error responses.
- Tower middleware for cross-cutting concerns.

Do not put business rules directly in route handlers.

---

## 7. DTO & Validation Standards

Use Serde for serialization and deserialization.

Use validator or custom validation functions for request validation.

Requirements:
- Separate Request DTOs from Domain Entities.
- Validate all external inputs.
- Validate nested objects when necessary.
- Apply appropriate length and range constraints.
- Reject malformed input.
- Avoid trusting client-provided identity fields.
- Never expose internal database structures directly.

Example:

```rust
use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    #[validate(email)]
    pub email: String,
}
```

Run validation explicitly before executing the use case.

Do not assume deserialization alone guarantees business validity.

---

## 8. Authentication & Authorization

Use middleware, extractors, and application-level authorization.

Requirements:
- Authenticate protected routes.
- Validate JWT signature, expiration, and claims.
- Enforce resource ownership.
- Verify roles and permissions.
- Derive current user identity from authenticated context.
- Never trust user IDs supplied by clients.
- Prevent unauthorized access to another user's resources.
- Hash passwords using Argon2.
- Never store plaintext passwords.
- Never hardcode secrets.

Keep authentication and authorization responsibilities separated.

Do not put all authorization logic into HTTP middleware if business-level resource permissions are required.

Project-specific rules:
- Every permission in §0.3 is checked in the Application layer and has tests for both the allowed and the denied case (owner vs. partner vs. other user).
- WebSocket connections authenticate with JWT when the connection opens and verify that the user belongs to the couple of that date plan (spec §8).
- Password-reset links are single-use and expire (spec §6.1).
- Invite codes expire after 24 hours and are single-use; a user belongs to at most one couple at a time (spec §6.2).

---

## 9. Database & SQLx Standards

Use PostgreSQL with SQLx.

SQLx is a SQL toolkit, not a traditional ORM.

Requirements:
- Use parameterized queries.
- Prefer compile-time checked query macros where practical.
- Use SQLx connection pools.
- Keep database access inside Infrastructure.
- Use migrations for schema changes.
- Use transactions for atomic operations.
- Define appropriate indexes and constraints.
- Handle unique and foreign key violations.
- Avoid N+1 queries.
- Use pagination for collection endpoints.
- Select only required columns.
- Avoid unnecessary database round trips.
- Never concatenate untrusted input into SQL strings.

Use separate structures for:
- Database models.
- Domain entities.
- API DTOs.

Do not expose SQLx database models directly as public API responses.

Use explicit mapping between layers when appropriate.

---

## 10. Error Handling

Use thiserror for typed application and domain errors.

Use anyhow for application startup, binaries, and error contexts where a concrete error type is unnecessary.

Define a consistent error structure.

Example categories:
- ValidationError
- NotFound
- Unauthorized
- Forbidden
- Conflict
- DatabaseError
- InternalError

Requirements:
- Do not expose database internals.
- Do not expose stack traces to clients.
- Preserve useful error context in logs.
- Map errors to appropriate HTTP status codes.
- Avoid silently swallowing errors.
- Use Result<T, E> consistently.
- Avoid unwrap() and expect() in production request paths.

Use IntoResponse to convert API errors into HTTP responses.

---

## 11. API Design Standards

Follow REST conventions.

Example:

```text
GET    /api/v1/users
GET    /api/v1/users/{id}
POST   /api/v1/users
PATCH  /api/v1/users/{id}
DELETE /api/v1/users/{id}

GET    /api/v1/activities
GET    /api/v1/activities/{id}
POST   /api/v1/activities
```

Requirements:
- Use consistent HTTP methods and status codes.
- Support pagination and filtering.
- Define request and response contracts.
- Generate OpenAPI documentation using Utoipa when appropriate.
- Return consistent error responses.
- Avoid exposing internal implementation details.
- Version public APIs when appropriate.

---

## 12. Testing Standards

Use Rust's built-in testing framework.

Use Tokio's test macro for asynchronous tests.

Testing levels:

**Unit Tests**
- Domain rules.
- Value objects.
- Application use cases.
- Error handling.

**Integration Tests**
- Repository implementations.
- PostgreSQL interactions.
- HTTP endpoints.
- Authentication and authorization workflows.

**End-to-End Tests**
- Important business workflows.
- Critical API scenarios.

Requirements:
- Test success and failure cases.
- Test edge cases.
- Test validation.
- Test permissions.
- Test transaction behavior when necessary.
- Use isolated test data.
- Clean up test resources.
- Never weaken tests just to make them pass.

Prefer real PostgreSQL integration tests for critical database behavior instead of relying exclusively on mocks.

---

## 13. Rust Coding Standards

Follow idiomatic Rust.

Mandatory principles:

- Use stable Rust.
- Follow ownership and borrowing principles.
- Prefer references over unnecessary cloning.
- Avoid unnecessary allocations.
- Use Result and Option appropriately.
- Prefer explicit error handling.
- Avoid excessive Arc and Mutex usage.
- Avoid blocking operations inside async handlers.
- Keep async functions focused.
- Use Send + Sync appropriately for shared state.
- Prefer immutable data where possible.
- Use traits when they provide meaningful abstraction.
- Avoid unnecessary dynamic dispatch.
- Avoid unsafe code unless absolutely necessary and explicitly justified.
- Avoid excessive unwrap() and expect() in production code.
- Use Clippy and rustfmt.

Naming conventions:

| Element | Convention |
|---|---|
| Files | snake_case.rs |
| Modules | snake_case |
| Functions | snake_case |
| Variables | snake_case |
| Structs | PascalCase |
| Enums | PascalCase |
| Traits | PascalCase |
| Constants | SCREAMING_SNAKE_CASE |
| Static variables | SCREAMING_SNAKE_CASE |

---

## 14. Observability & Logging

Use tracing and tracing-subscriber.

Requirements:
- Use structured logging.
- Include request correlation IDs where appropriate.
- Record important business events.
- Avoid logging passwords, tokens, and sensitive information.
- Use appropriate log levels.
- Capture errors with useful context.
- Configure graceful shutdown.
- Configure request timeouts and body limits.

Do not use println! as the production logging mechanism.

---

## 15. Engineering Workflow

Before implementation:

1. Inspect the existing codebase.
2. Read Cargo.toml and project configuration.
3. Understand module boundaries.
4. Understand business requirements and acceptance criteria.
5. Inspect existing database schemas and migrations.
6. Identify affected use cases.
7. Identify security and data integrity risks.
8. Propose a plan for complex changes.

During implementation:

1. Follow existing conventions.
2. Make changes within the requested scope.
3. Reuse existing functionality.
4. Keep modules independent.
5. Implement business rules in the correct layer.
6. Add or update tests.
7. Avoid unnecessary refactoring.

After implementation, run (backend, from `backend/`):

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check
cargo test
cargo build
```

Frontend commands (lint, type check, test, build) are defined when the frontend is scaffolded; record them here at that point.

Use commands appropriate to the actual project configuration.

Also:
- Review the final diff.
- Verify migrations.
- Verify API behavior.
- Report completed changes.
- Report failed or skipped checks.
- Never claim a check passed without executing it.

---

## 16. Anti-Overengineering Rules

These rules are mandatory:

- Prefer Modular Monolith.
- Do not introduce microservices by default.
- Do not force CQRS on simple CRUD.
- Do not create traits for every struct.
- Do not create unnecessary repository abstractions.
- Do not create unnecessary generic frameworks.
- Do not use Arc<Mutex<T>> without a clear reason.
- Do not introduce event sourcing without explicit requirements.
- Do not add Redis or message queues without a real use case.
- Do not add unnecessary crates.
- Do not create empty modules.
- Do not refactor unrelated functionality.
- Do not imitate NestJS architecture mechanically.

Prefer the simplest maintainable Rust implementation.

---

## 17. Architecture Decision Rules

For every new project:

1. Identify business domains.
2. Define module boundaries.
3. Define core entities and relationships.
4. Design database schema.
5. Define authentication and authorization.
6. Define API contracts.
7. Identify important use cases.
8. Implement the core application.
9. Add tests.
10. Document important architectural decisions.

If an existing codebase uses a different architecture, inspect and respect its conventions before changing anything.

When a requirement conflicts with this document, explain the trade-offs and recommend an appropriate solution.

Record approved architectural decisions in `docs/` (spec or a decision log), not only in this file or in memory.

---

## 18. Frontend Standards

Only spec-backed rules are listed here. Extend this section after the frontend stack details (§2.2) are approved.

- Mobile-first layout; the main target is a phone screen.
- The pixel cat sprite renders crisp with `image-rendering: pixelated`, about 96px wide on phones, peeking from the upper half of the left edge (spec §6.7). It must never cover a large part of the screen.
- The admin area lives in the same app under `/admin`.
- Never render data the current user is not allowed to see, even if hidden by CSS — the API must not send it in the first place (§0.3).
- After a WebSocket reconnect, reload the full date-plan state from the API; PostgreSQL is the source of truth (spec §8).

---

Final principle:

Build reliable, idiomatic, maintainable Rust software. Prefer clear code and explicit behavior over architectural complexity.
