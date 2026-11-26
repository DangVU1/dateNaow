# dateNaow — Decision Log

Each entry: date, decision, rationale, and source. Superseded entries stay in place, marked **Superseded by #N**. The detailed content of each decision lives in the referenced document; this log records what was decided, when, and why.

---

## #1 — Recommendation approach: curated catalogue + rules (Hướng 2)

- **Date:** 2026-11-23
- **Decision:** Places come from a catalogue curated by admins (Saigon only at first). Gifts are chosen by rule-based scoring. Free-text wishes and notes are analysed with a keyword dictionary, without AI/LLM.
- **Options considered:** (1) Google Places + rules; (2) curated catalogue + rules; (3) Google Places + LLM.
- **Rationale:** accurate prices and date-appropriate places, no per-call API cost, testable logic, and no private data sent to third parties. Trade-off: admins must enter data manually, and coverage starts with Saigon only.
- **Source:** spec §4.

## #2 — Technology stack

- **Date:** 2026-11-23
- **Decision:** Backend Rust + Axum + Tokio + PostgreSQL + SQLx, modular monolith with Clean Architecture; frontend React + Vite + TypeScript; email/password login with JWT and Argon2; Docker Compose for local development.
- **Rationale:** chosen by the project owner.
- **Source:** spec §3, `.claude/CLAUDE.md` §2.

## #3 — Random gift suggestions before a date: buttons and frequency

- **Date:** 2026-11-25
- **Decision:** The speech bubble has four buttons: Chốt món này / Gợi ý khác / Mình tự lo quà rồi / Không tặng quà lần này. The cat suggests at most **3 times per date**, spread evenly over the time from when the date gets a day until the end of that day (three equal windows, at most once per day).
- **Options considered:** at most once per day until the date (Claude's first proposal); once every two days; a total cap.
- **Rationale:** the project owner chose a total cap of 3, spread over the remaining time, so a date planned far ahead does not mean daily interruptions.
- **Source:** spec §7.3.

## #4 — Design spec approved in full

- **Date:** 2026-11-25
- **Decision:** The project owner approved the whole design spec, including every item previously labelled [Đề xuất] (Claude's proposals), the module split (spec §8) and the repository root layout (`.claude/CLAUDE.md` §4). The spec status changed from "bản nháp, chờ duyệt" to "đã duyệt".
- **Rationale:** brainstorming and builder check were done; there are no open questions about requirements. Items in spec §9 remain for the implementer to propose.
- **Source:** `docs/superpowers/specs/2026-11-23-datenaow-design.md`.

## #5 — Implementation split into four plans

- **Date:** 2026-11-25
- **Decision:** Implementation is split into four plans, each ending in working, tested software: (1) foundation + accounts, (2) couples + partner notes + wishes + text analysis, (3) places + date plans + place-picking chat, (4) gifts + occasions + the cat. A short roadmap covers all four. Only the next plan is written in detail, just before it is built, so it can use the real interfaces of the code already written.
- **Options considered:** roadmap + detailed plan 1 (chosen); all four plans in detail now; one single plan.
- **Rationale:** smaller plans are easier to review, and later plans do not drift from code that does not exist yet.
- **Source:** `docs/superpowers/plans/2026-11-25-datenaow-roadmap.md`.

## #6 — Frontend libraries and structure

- **Date:** 2026-11-25
- **Decision:** React Router v8 in library (declarative) mode; TanStack Query v5 for server state; React Context + `useReducer` for the auth session and UI state (no global store library); feature-based folders (`src/app/`, `src/features/<feature>/`, `src/shared/`); ESLint (flat config, typescript-eslint, react-hooks rules) + Prettier; Vitest + React Testing Library + jsdom for tests. Playwright end-to-end tests are deferred to plan 3.
- **Options considered:** TanStack Router + Query; React Router + hand-written fetch; layer-based folders (`pages/`, `components/`, `hooks/`); Biome instead of ESLint + Prettier.
- **Rationale:** React Router is the simplest and best documented router for a learner; TanStack Query avoids hand-writing caching, retries and loading states; feature folders mirror the backend modules; react-hooks lint rules catch common React mistakes.
- **Source:** spec §9 (left to the implementer), `.claude/CLAUDE.md` §2.2.

## #7 — Extra backend crates, date/time library, login rate limiting, PostgreSQL version

- **Date:** 2026-11-25
- **Decision:** Besides the stack in decision #2, the backend may use tower-http, tracing-subscriber, axum-extra (cookies), uuid, dotenvy and chrono. Failed-login limits (per IP and per email, spec §6.1) use a small counter table in PostgreSQL, written in the project, with no rate-limiting crate. PostgreSQL 18 is the database version.
- **Options considered:** chrono + tower_governor; `time` + hand-written counter. PostgreSQL 19 (released 2026-11-19) was not chosen yet because it is only days old.
- **Rationale:** chrono has the most examples with sqlx. A Postgres-backed counter covers both IP and email limits, survives restarts and is easy to test. PostgreSQL 18 is mature and supported until 2030-11.
- **Source:** owner's choice on 2026-11-25; version facts gathered by the `researcher` subagent on 2026-11-25.

## #8 — Keep both test-driven-development skills

- **Date:** 2026-11-25
- **Decision:** Keep the project copy `.claude/skills/test-driven-development` and the Superpowers plugin's `superpowers:test-driven-development`. Neither is deleted.
- **Rationale:** both follow the same red-green-refactor cycle and do not conflict. The project copy matches `.claude/CLAUDE.md` §12 (discover the repo's test command, test pyramid, real PostgreSQL). The plugin's execution skills call their own copy, and one plugin skill cannot be removed without disabling the whole plugin.
- **Source:** owner's choice on 2026-11-25.

## #9 — Plan 1 proposals P1–P13 approved

- **Date:** 2026-11-25
- **Decision:** The owner approved all proposals P1–P13 in `docs/superpowers/plans/2026-11-25-plan-1-foundation-accounts.md` ("Đề xuất cần duyệt cùng kế hoạch") without changes. In short:
  - **P1 — database conventions:** plural snake_case tables; `UUID DEFAULT uuidv7()` keys; `timestamptz`; TEXT + CHECK instead of Postgres ENUM; `idx_`/`uq_` names; VND as BIGINT.
  - **P2 — API conventions:** `/api/v1` prefix; snake_case JSON; one error envelope `{"error":{"code","message","fields"}}` with a fixed status-code table; `Cache-Control: no-store` on auth and `/me` responses.
  - **P3 — sessions:**
    - HS256 JWT access token, valid for 15 minutes.
    - Opaque 32-byte refresh token, stored only as a SHA-256 hash.
    - Each refresh rotates the token and extends its lifetime to 30 days from that moment.
    - Reusing a replaced token more than 30 seconds after the replacement revokes all of the user's sessions; reuse within 30 seconds returns 409 `refresh_conflict` and revokes nothing.
    - Every refresh 401 clears the cookie.
    - Cookie: `dn_refresh`, HttpOnly, SameSite=Strict, Path=/api/v1/auth.
    - Signing up also signs the user in.
  - **P4 — passwords:** 8–128 characters with no composition rules; Argon2id with the crate's default parameters, configurable. *Amended by #16:* common passwords are also rejected. *Amended by #17:* passwords are normalised to NFC.
  - **P5 — attempt limits:** login 5 failures per email and 20 per IP in 15 minutes; password reset 3 per email and 10 per IP per hour. Attempts are recorded before they are counted; IPv6 is limited per /64; limiter keys are hashed; `X-Forwarded-For` is read only when `TRUST_PROXY=true`. *Amended by #13:* the per-email limits are replaced by limits per (email, IP) pair plus a looser global per-email cap. *Amended by #17:* a sign-up limit per IP, and only the rightmost `X-Forwarded-For` entry is used.
  - **P6 — password reset:** single-use link valid for 30 minutes, carrying the token in the URL fragment. The request endpoint always returns 204 and sends the email in a background task. Completing a reset revokes all refresh tokens. SMTP is plaintext to Mailpit only.
  - **P7 — admin bootstrap:** the `make_admin` CLI; admin checks re-read the role from the database.
  - **P8:** OpenAPI is served as JSON only.
  - **P9:** 15-second timeout, 64 KiB body limit, `x-request-id` header.
  - **P10:** CSS Modules; English route paths; Vietnamese UI copy.
  - **P11:** generic repository traits; `dyn` for Clock and EmailSender; concrete Argon2 and JWT services; the password reset transaction calls `PgUserRepository::set_password_hash_in_tx`.
  - **P12:** registering an existing email returns 409 `email_taken` (accepted, since v1 has no email verification).
  - **P13 — extra dependencies:**
    - backend: serde_json, rand, sha2, tower; http-body-util (dev only);
    - frontend (dev only): @testing-library/user-event, @testing-library/jest-dom, jsdom, typescript-eslint, @eslint/js, eslint-plugin-react-hooks, eslint-plugin-react-refresh, eslint-config-prettier, globals;
    - global tool: sqlx-cli.
- **Rationale:** see the "Lý do / ví dụ" column for each item in the plan. The plan was reviewed by the `reviewer` subagent before approval, and its findings were fixed.
- **Source:** `docs/superpowers/plans/2026-11-25-plan-1-foundation-accounts.md`.

## #10 — Plan 1 execution method

- **Date:** 2026-11-25
- **Decision:** Plan 1 is executed subagent-driven: a fresh subagent implements each task, a separate reviewer checks it before the next task starts, and the whole branch is reviewed at the end.
- **Options considered:** subagent-driven (chosen); native (one session implements all tasks, with a single review at the end).
- **Rationale:** the tasks depend tightly on each other's interfaces, and the authentication code is security-sensitive, so catching mistakes task by task is worth the extra cost.
- **Source:** owner's choice on 2026-11-25.
- **Addendum (2026-11-25):** the Superpowers plugin (`superpowers:subagent-driven-development`) is not available in the executing session, so the owner chose to run the same pattern by hand: the main agent launches one implementer subagent per task, then a separate reviewer subagent, and integrates and commits itself. For plan 1 this is a deliberate exception to the global rule that only the main agent edits files.

## #11 — Extra frontend dev dependency: @testing-library/dom

- **Date:** 2026-11-25
- **Decision:** add `@testing-library/dom@10.4.2` as an exact-pinned devDependency of `frontend/`.
- **Options considered:** approve it (chosen); drop React Testing Library, which would change #6.
- **Rationale:** `@testing-library/react` 16.3.3, `@testing-library/user-event` 14.6.13 and `@testing-library/jest-dom` 8.0.0 (approved in #6 / P13) all declare it as a required peer dependency, so the approved test stack does not work without it. It is MIT-licensed and maintained by the same project. Found in plan 1, Task 11.
- **Source:** owner's approval in chat on 2026-11-25.

## #12 — TypeScript 6.0.3 instead of 7.0.2 (version fallback)

- **Date:** 2026-11-25
- **Decision:** pin `typescript@6.0.3` in `frontend/` instead of the planned 7.0.2.
- **Options considered:** TypeScript 7.0.2 (planned); 6.0.3, the newest version typescript-eslint supports (chosen).
- **Rationale:** `typescript-eslint` 8.74.0 declares the peer range `typescript >=4.8.4 <6.1.0`, so TS 7 would break type-aware linting. This applies the version fallback pre-approved in plan 1 ("Phiên bản và phụ thuộc"); no new approval was needed. Revisit when typescript-eslint supports TS 7.
- **Source:** plan 1, Task 11; recorded in `.claude/CLAUDE.md` §2.4.

## #13 — Failed-attempt limits keyed by email + IP (amends P5)

- **Date:** 2026-11-25
- **Decision:** count failed sign-ins per (email, IP) pair — at most 5 per 15 minutes — plus a looser global cap of 50 per email per 15 minutes, keeping 20 per IP per 15 minutes. Password-reset requests are likewise limited per (email, IP) pair (3 per hour) plus the existing 10 per IP per hour and a global cap of 10 per email per hour (the exact reset numbers are Claude's proposal within the chosen option). IPv6 keeps the /64 grouping; keys stay SHA-256 hashed. Supersedes the per-email-only limits in P5 (#9).
- **Options considered:** keep per-email-only limits and correct the P5 risk wording; key by email + IP with a global per-email cap (chosen).
- **Rationale:** the final security review of plan 1 found that, with per-email limits, a stranger sending ~6 wrong passwords every 15 minutes from one IP could lock an account out indefinitely — the correct password was refused too — and the same IP could also exhaust the per-email reset-request limit, blocking recovery. Keying by email + IP stops a single IP from doing that; the global cap still bounds guessing from many IPs (at most 50 guesses per 15 minutes per email). *(Wording corrected on 2026-11-25, see #15: the original said keying by email + IP "stops that" outright.)*
- **Source:** owner's choice in chat on 2026-11-25 (final review finding F2).

## #14 — Forgot-password page shows errors when nothing was sent (amends plan 1, Task 14)

- **Date:** 2026-11-25
- **Decision:** the forgot-password page shows the same confirmation for every 204 response; on 429 it shows the rate-limit message; on a network error, a server error or an invalid email it shows an error message instead of the confirmation.
- **Options considered:** always show the confirmation except on 429 (plan wording "luôn"); show errors when nothing was sent (chosen).
- **Rationale:** confirming "check your inbox" when the request never reached the server misleads the user. The backend's 204 does not depend on whether the account exists, and the review confirmed these error cases reveal nothing about which emails have accounts.
- **Source:** owner's choice in chat on 2026-11-25.

## #15 — Residual lockout risk of #13 accepted for v1

- **Date:** 2026-11-25
- **Decision:** keep the #13 limits as implemented (commit `7589d17`) and accept the residual risk: an attacker with about 10 distinct IPv4 addresses or IPv6 /64s can keep one email's sign-in locked (global cap of 50), and about 4 can block its reset requests (global cap of 10). The #13 rationale was corrected so it no longer claims the lockout is stopped outright. Follow-up before public launch: a "known device" bypass, where a signed cookie set after a successful sign-in lets that device skip the per-email global cap.
- **Options considered:** accept and correct the wording (chosen); a known-device bypass now; count IPv6 pairs per /56.
- **Rationale:** v1 is a small couples app with little incentive for a sustained multi-IP attack. The bypass is the strongest fix but adds a task's worth of code, and /56 grouping does not help against IPv4 address pools. Review of #13, finding D1.
- **Source:** owner's choice in chat on 2026-11-25.

## #16 — Reject common passwords (amends P4)

- **Date:** 2026-11-25
- **Decision:** sign-up and password reset reject passwords found in a list of about 10,000 common passwords. The list is embedded in the backend as a data file, with no new crate. List source (owner-approved in chat on 2026-11-25): SecLists `Passwords/Common-Credentials/10k-most-common.txt`, pinned at commit `e458f754946ed0accb41744db3f71f3a3bbcdbc8` (SHA-256 `68782d6a4a19a4768d5f15dd66bd534e7a33055cc755411e33f16d18c50fdcce`), MIT licence kept next to it as `SECLISTS_LICENSE`. A short hand-written list of common Vietnamese passwords (`vietnamese.txt`, Claude's proposal) is added alongside; the owner reviews it with the code. Sign-in does not check the list, so guesses at common passwords are still counted by the attempt limiter.
- **Options considered:** block common passwords (chosen); accept 50 guesses per 15 minutes per email as decided in #13; lower the global cap to 20.
- **Rationale:** with #13, an attacker using many IPs gets up to 50 guesses per 15 minutes per email (about 4,800 a day, ten times the old P5). Blocking common passwords removes the passwords such guessing finds, and adds no new lockout risk. Lowering the cap would make lockout cheaper (#15), and a daily cap would let a single IP fill it, bringing F2 back. Review of #13, finding D2.
- **Source:** owner's choice in chat on 2026-11-25.

## #17 — Plan 1 review follow-ups (amends P4, P5 and plan 1 Task 7)

- **Date:** 2026-11-26
- **Decision:** before PR #1 is merged, the open items from the plan 1 reviews are settled as follows.
  - **Sign-up limit (amends P5):** at most 10 sign-up attempts per IP address (IPv6: per /64) per hour, using the existing attempt limiter (record first, count second, hashed keys, 429 with `Retry-After`).
  - **`X-Forwarded-For` (amends P5 wording):** with `TRUST_PROXY=true`, only the rightmost entry is used. If it is not a valid address, the socket address is used; entries further left are never used. A warning is logged once if a request carries `X-Forwarded-For` while `TRUST_PROXY` is off.
  - **Logout (amends plan 1 Task 7):** if the refresh token cannot be revoked because of a storage error, logout answers 500 and still clears the cookie, instead of always answering 204.
  - **Unicode passwords (amends P4):** passwords are normalised to NFC before every use (sign-up, password reset, sign-in), so the same password typed on two devices matches. The length rules and the common-password list (#16) apply to the normalised form. Uses the `unicode-normalization` crate, already in the dependency tree through sqlx.
  - **Other fixes:** a wrong HTTP method answers 405 with the JSON error envelope (`method_not_allowed`). Concurrent reset requests leave only one live reset link. Code that sets a new password requires a `NewPassword` type, which only the common-password check can produce. An index on `auth_attempts.attempted_at` serves the prune query. Plan 1 notes that the refresh flow loads the user before rotating the token.
  - **Kept as they are:** the ~1 ms timing difference between known and unknown emails on reset requests; the extra query when a reset is confirmed; shared-IP limits (D3); a reset clearing only the resetting IP's pair (D4).
- **Options considered:** for each item, fix now or defer. For logout: always 204 (plan) or 500 with the cookie cleared (chosen). For Unicode: NFC now (chosen) or record as a known limitation.
- **Rationale:** each fix is small and closes a gap found in review before real users exist. Normalising passwords is free before launch but would need every stored hash migrated after it. A 500 on a failed logout tells the user truthfully that the session may still be live.
- **Source:** owner's choices in chat on 2026-11-26. For the two fix lists the owner accepted Claude's recommendation to do all eight.

