# Kế hoạch 1: Nền móng và tài khoản — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dựng khung backend và frontend của dateNaow, rồi làm trọn phần tài khoản. Người dùng đăng ký, đăng nhập, giữ phiên đăng nhập, đăng xuất, đặt lại mật khẩu qua email và chọn giới tính được. Trang `/admin` chỉ mở với vai trò `admin`.

**Architecture:**
- Backend Rust/Axum là một modular monolith theo Clean Architecture (`.claude/CLAUDE.md` §3-§4). Kế hoạch này tạo hai module `users` và `auth`, cùng phần dùng chung `config`, `shared` và `infrastructure`.
- Frontend React/Vite chạy cùng origin với API. Khi phát triển, Vite chuyển `/api` sang backend.
- Access token JWT chỉ nằm trong bộ nhớ của trình duyệt. Refresh token nằm trong cookie httpOnly.

**Tech Stack:** Rust stable, Axum 0.8, Tokio, SQLx 0.9 + PostgreSQL 18, jsonwebtoken, argon2, lettre (Mailpit), utoipa, tracing; React 19, Vite 8, TypeScript, React Router 8 (library mode), TanStack Query 5, Vitest + React Testing Library, ESLint + Prettier.

**Spec:** `docs/superpowers/specs/2026-11-23-datenaow-design.md`, chủ yếu §3, §6.1 và §8. Lộ trình: `docs/superpowers/plans/2026-11-25-datenaow-roadmap.md`.

---

## Cách đọc kế hoạch này

- **Quyết định của chủ dự án**, không bàn lại: spec đã duyệt, `.claude/CLAUDE.md`, và `docs/DECISIONS.md` từ #1 đến #8. Trong đó có stack, cách chia module, thư viện frontend, các crate bổ sung, bộ đếm đăng nhập sai bằng PostgreSQL và PostgreSQL 18.
- **Đề xuất của Claude:** những gì spec để ngỏ (spec §9) hoặc không nói tới. Mỗi đề xuất có mã **P1, P2…** ở mục "Đề xuất cần duyệt" bên dưới. Chủ dự án duyệt kế hoạch tức là duyệt các đề xuất này. Các đề xuất được ghi vào `docs/DECISIONS.md` ngay khi được duyệt (Task 0).
- Mã code (tên hàm, tên bảng, tên file) viết bằng tiếng Anh. Chữ hiện trên giao diện viết bằng tiếng Việt, giống spec.

## Đề xuất cần duyệt cùng kế hoạch

Bạn có thể đồng ý cả bảng hoặc phản đối từng mục. Mục nào bị từ chối thì mình sửa kế hoạch rồi trình lại phần đó.

### Nhóm A: ảnh hưởng tới người dùng hoặc chi phí

| Mã | Đề xuất | Lý do / ví dụ | Nếu bạn không đồng ý |
|---|---|---|---|
| **P3** | **Phiên đăng nhập.** Access token là JWT ký HS256, sống 15 phút, chứa `sub` (id người dùng), `role`, `iat`, `exp`. Refresh token là chuỗi ngẫu nhiên 32 byte, trong DB chỉ lưu mã băm SHA-256. Mỗi lần refresh thì nhận token mới sống thêm **30 ngày tính từ lúc refresh**, còn token cũ bị thu hồi. Như vậy ai mở app ít nhất 30 ngày một lần sẽ không phải đăng nhập lại. Token cũ mà bị dùng lại **sau** 30 giây kể từ lúc được thay thì bị coi là bị đánh cắp, và mọi phiên của người đó bị thu hồi. Dùng lại **trong vòng** 30 giây thì trả 409 `refresh_conflict`, không thu hồi gì, vì trường hợp này thường là hai tab cùng refresh một lúc. Token bị thu hồi vì lý do khác (đăng xuất, đổi mật khẩu) thì chỉ trả 401, không thu hồi dây chuyền. Mọi lần refresh thất bại với 401 đều xóa cookie. Cookie: `dn_refresh`, `HttpOnly`, `SameSite=Strict`, `Path=/api/v1/auth`, `Max-Age=2592000`, có `Secure` khi `COOKIE_SECURE=true`. Đăng ký xong thì đăng nhập luôn. | Chỉ lưu mã băm thì dù lộ DB, kẻ gian cũng không dùng được token. Ví dụ hai tab: tab A vừa đổi token, tab B vẫn gửi token cũ trong cùng giây đó. Nếu không có 30 giây khoan dung thì cả hai tab bị đăng xuất oan. | Có thể bỏ phần phát hiện token bị đánh cắp cho đơn giản. Đổi lại, token bị lộ sẽ dùng được đến khi hết hạn. |
| **P5** | **Giới hạn số lần thử sai.** Đăng nhập: tối đa 5 lần sai cho mỗi email và 20 lần sai cho mỗi IP trong 15 phút. Yêu cầu đặt lại mật khẩu: 3 lần cho mỗi email và 10 lần cho mỗi IP trong 1 giờ. Vượt giới hạn thì trả 429 kèm `Retry-After` bằng cả khung thời gian. Hệ thống **ghi lần thử trước rồi mới đếm**, nên 100 lần đoán gửi cùng lúc cũng chỉ có 5 lần được kiểm tra mật khẩu. Đăng nhập đúng hoặc đổi mật khẩu xong thì xóa bộ đếm của email đó. Địa chỉ IPv6 được tính theo cả dải /64. Khóa đếm được băm SHA-256 trước khi lưu. Chỉ đọc `X-Forwarded-For` (lấy địa chỉ ngoài cùng bên phải) khi `TRUST_PROXY=true`. | Băm khóa nghĩa là bảng đếm không chứa email hay IP dạng chữ đọc được. **Rủi ro chấp nhận:** người lạ cố tình nhập sai 5 lần có thể khóa đăng nhập của email bạn trong 15 phút. | Nới các con số (ví dụ 10 lần cho mỗi email), hoặc chỉ giới hạn theo IP. **Đã sửa theo `docs/DECISIONS.md` #13:** đăng nhập 5 lần sai cho mỗi cặp (email, IP), 20 cho mỗi IP, 50 cho mỗi email (tổng mọi IP) trong 15 phút; yêu cầu đặt lại 3 cho mỗi cặp, 10 cho mỗi IP, 10 cho mỗi email trong 1 giờ. Giới hạn theo email chỉ được tính khi lần thử đã qua giới hạn cặp và IP, nên một IP không thể khóa tài khoản của người khác. |
| **P6** | **Đặt lại mật khẩu.** Link sống 30 phút và chỉ dùng một lần. Yêu cầu link mới thì link cũ hết hiệu lực. Link có dạng `{APP_BASE_URL}/reset-password#token=...`: token nằm sau dấu `#` nên không bị gửi lên server và không lọt vào log. Yêu cầu đặt lại luôn trả 204, kể cả khi email không có tài khoản. Email được gửi ở tác vụ nền để thời gian phản hồi không để lộ email nào có tài khoản. Đổi mật khẩu xong thì thu hồi mọi refresh token. Access token đang dùng vẫn còn hiệu lực tối đa 15 phút. Kế hoạch 1 chỉ gửi SMTP không mã hóa tới Mailpit. TLS cho nhà cung cấp SMTP thật làm ở kế hoạch triển khai. | Luôn trả 204 để không ai dò được email nào đã có tài khoản qua màn hình quên mật khẩu. | Đổi thời hạn link, ví dụ 60 phút. |
| **P12** | **Đăng ký bằng email đã có** thì trả 409 `email_taken`. | Lỗi này cho người khác biết email đó đã có tài khoản. Bản đầu cũng không xác minh email, nên ai đó có thể đăng ký bằng email của người khác. Muốn chặn cả hai việc này thì phải gửi email xác minh, mà spec chỉ cho gửi email khi đặt lại mật khẩu. | Thêm bước xác minh email. Việc này là thay đổi spec và cần ghi vào `docs/DECISIONS.md`. |
| **P13** | **Phụ thuộc cần duyệt thêm.** Backend: `serde_json` (đọc/ghi JSON, Axum đã dùng sẵn bên trong), `rand` (sinh token ngẫu nhiên an toàn), `sha2` (băm token), `tower` (timeout, và gọi router trong test; Axum đã dùng sẵn bên trong), `http-body-util` (chỉ dùng trong test, để đọc body). Phần mã hóa hex tự viết, không thêm crate `hex`. Frontend (chỉ dùng khi phát triển): `@testing-library/user-event`, `@testing-library/jest-dom`, `jsdom`, `typescript-eslint`, `@eslint/js`, `eslint-plugin-react-hooks`, `eslint-plugin-react-refresh`, `eslint-config-prettier`, `globals`. Công cụ cài toàn máy: `sqlx-cli`, dùng để tạo cache truy vấn cho bản build Docker. | Theo quy tắc toàn cục, thêm dependency hay cài công cụ cho cả máy đều cần chủ dự án duyệt. Các crate trên đều là thư viện phổ biến và được bảo trì đều. | Không có `sqlx-cli` thì bản build Docker phải kết nối được DB lúc biên dịch. |

### Nhóm B: quy ước kỹ thuật (người dùng không thấy)

| Mã | Đề xuất | Lý do / ví dụ |
|---|---|---|
| **P1** | **Quy ước cơ sở dữ liệu.** Tên bảng ở dạng số nhiều, `snake_case` (`users`, `refresh_tokens`). Khóa chính là `id UUID DEFAULT uuidv7()`, dùng hàm có sẵn của PostgreSQL 18. Thời gian dùng `timestamptz`. Giá trị liệt kê lưu dạng `TEXT` kèm `CHECK`, không dùng kiểu `ENUM` của Postgres. Index đặt tên `idx_<bảng>_<cột>`, ràng buộc unique đặt `uq_<bảng>_<cột>`. Tên file migration bắt đầu bằng timestamp. Tiền VND (từ kế hoạch 2) lưu bằng `BIGINT`. | UUIDv7 tăng dần theo thời gian nên index gọn hơn UUID ngẫu nhiên. Dùng `TEXT` + `CHECK` thì muốn thêm giá trị mới chỉ cần sửa một dòng `CHECK`. |
| **P2** | **Quy ước API.** Mọi đường dẫn có tiền tố `/api/v1`. JSON dùng `snake_case`. Lỗi luôn có dạng `{"error":{"code":"...","message":"...","fields":{...}}}`. Mã HTTP: 400 JSON hỏng hoặc sai kiểu, 401 chưa đăng nhập hoặc sai thông tin, 403 không đủ quyền, 404, 408 quá thời gian, 409 trùng hoặc xung đột, 413 body quá lớn, 422 dữ liệu sai luật kèm `fields`, 429 kèm `Retry-After`, 500 chỉ trả câu chung chung. Phản hồi của `/api/v1/auth/*` và `/api/v1/me` có `Cache-Control: no-store`. | Frontend chỉ cần đọc `code` (và tên trường) để hiện câu tiếng Việt phù hợp. Ví dụ `email_taken` thì hiện "Email này đã có tài khoản.". |
| **P4** | **Mật khẩu** dài 8-128 ký tự (đếm ký tự, không đếm byte). Không bắt buộc phải có chữ hoa hay ký hiệu. Argon2id dùng tham số mặc định của crate argon2 (khoảng 19 MiB, 2 vòng, 1 luồng), chỉnh được qua cấu hình. Khi test thì dùng tham số nhỏ để chạy nhanh. Việc băm chạy trong `spawn_blocking`. | Đây là cách NIST khuyên: mật khẩu dài quan trọng hơn mật khẩu phức tạp. Giới hạn 128 ký tự cùng giới hạn kích thước body chặn được trường hợp gửi mật khẩu thật dài để làm server băm rất lâu. |
| **P7** | **Tạo admin** bằng lệnh `cargo run --bin make_admin -- <email>`. Khi gọi API quản trị, hệ thống đọc lại vai trò từ DB chứ không tin vai trò ghi trong JWT. | Không có trang đăng ký admin, nên không ai tự phong mình làm admin được. Khi một admin bị hạ quyền, việc hạ quyền có hiệu lực ngay. |
| **P8** | **OpenAPI:** chỉ phục vụ file JSON tại `/api/v1/openapi.json`. Chưa dùng Swagger UI. | Researcher chưa xác nhận được `utoipa-swagger-ui` và `utoipa-axum` chạy được với utoipa 6. Muốn có giao diện xem tài liệu thì duyệt thêm sau. |
| **P9** | **Middleware:** timeout 15 giây (trả lỗi JSON 408), body tối đa 64 KiB qua `DefaultBodyLimit` của axum (trả lỗi JSON 413; kế hoạch 3 sẽ nới riêng cho route tải ảnh), header `x-request-id` cho mỗi request và ghi vào log của request đó. | |
| **P10** | **Frontend.** Dùng CSS Modules, có sẵn trong Vite nên không cần thư viện. Đường dẫn trang bằng tiếng Anh (`/sign-in`, `/sign-up`, `/forgot-password`, `/reset-password`, `/settings`, `/admin`). Các câu tiếng Việt trên giao diện được ghi trong từng task. | |
| **P11** | **Cách tổ chức code.** Repository là trait với `async fn` có sẵn của Rust, use case nhận repository qua generic. `Clock` và `EmailSender` dùng `Arc<dyn ...>` để test thay bằng đồng hồ giả và hộp thư giả. Argon2 và JWT là struct cụ thể, không tạo trait. Khi đổi mật khẩu bằng link đặt lại, `auth` mở một transaction và gọi hàm `PgUserRepository::set_password_hash_in_tx` của module `users`. Câu SQL của bảng `users` vẫn nằm trong module `users`. | Không tạo trait cho mọi thứ (CLAUDE.md §16). Transaction đảm bảo "đánh dấu link đã dùng + đổi mật khẩu + thu hồi phiên" hoặc xảy ra cả ba, hoặc không xảy ra việc nào. |

## Global Constraints

Mọi task đều phải tuân theo các ràng buộc sau, dù task đó không nhắc lại.

- Access token sống ngắn, khoảng 15 phút. Refresh token sống dài, khoảng 30 ngày, nằm trong cookie httpOnly và thu hồi được (spec §6.1).
- Link đặt lại mật khẩu chỉ dùng một lần và có hạn dùng (spec §6.1).
- Mật khẩu được băm bằng Argon2, không bao giờ lưu dạng chữ thường (spec §6.1, CLAUDE.md §8).
- Giới hạn số lần đăng nhập sai theo IP và theo email (spec §6.1).
- Giới tính có bốn lựa chọn: Nam, Nữ, Khác, Không muốn nói. Mục này không bắt buộc, đổi được bất cứ lúc nào, và **không bao giờ hiện cho người kia** (spec §6.1).
- Chỉ gửi email khi đặt lại mật khẩu. Gửi qua `lettre`, môi trường phát triển dùng Mailpit (spec §2, §6.1).
- Frontend và API chạy cùng origin. Cookie refresh dùng `SameSite=Strict`, không cấu hình CORS. Khi phát triển, Vite chuyển `/api` sang backend (spec §8).
- Trang quản trị nằm ở `/admin` trong cùng ứng dụng. API quản trị bắt buộc vai trò `admin` (spec §8).
- Không làm đăng nhập Google hoặc OTP, không gửi thông báo đẩy, không gửi email nhắc (spec §2).
- Tầng Domain không phụ thuộc Axum, SQLx hay HTTP (CLAUDE.md §3.2).
- Không dùng `unwrap()` hoặc `expect()` trên đường xử lý request. Lỗi có kiểu cụ thể, viết bằng `thiserror`. `anyhow` chỉ dùng lúc khởi động và trong các binary (CLAUDE.md §10).
- Ghi log bằng `tracing`, không dùng `println!` trong server. Không bao giờ ghi mật khẩu, token hoặc mã băm vào log (CLAUDE.md §14).
- Truy vấn SQL luôn có tham số, ưu tiên macro `query!` hoặc `query_as!` được kiểm tra lúc biên dịch. Schema chỉ đổi qua migration (CLAUDE.md §9).
- Phụ thuộc chỉ gồm các thứ đã duyệt (`docs/DECISIONS.md` #2, #6, #7) và P13. Phiên bản được ghim chính xác, lockfile được commit.
- Chạy sau mỗi task backend, trong thư mục `backend/`: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`. Từ Task 10 trở đi, chạy thêm `cargo sqlx prepare --check`. Chạy sau mỗi task frontend, trong `frontend/`: `npm run lint`, `npm run typecheck`, `npm test`.
- Giao diện ưu tiên màn hình điện thoại (khoảng 375px). Chữ trên giao diện viết bằng tiếng Việt.
- Làm trên nhánh `feat/plan-1-foundation-accounts`. Mỗi bước "Commit" chỉ chạy khi chủ dự án đã đồng ý. Chủ dự án có thể đồng ý một lần cho cả kế hoạch lúc chọn cách thực thi. Không push khi chưa được duyệt riêng.

## Review Focus

Năm tình huống spec không nói thẳng nhưng dễ làm người dùng thật gặp lỗi nhất, xếp theo khả năng xảy ra. Mỗi tình huống có test ở task được ghi kèm.

1. **Hai tab cùng refresh một lúc.** Người dùng mở app ở hai tab, access token hết hạn và cả hai tab cùng gọi refresh. Kết quả mong đợi: không tab nào bị đăng xuất. Test ở Task 7 (`refresh_with_old_cookie_within_30s_returns_409_refresh_conflict_and_keeps_session`) và Task 12 (`refresh_conflict_is_retried_once_after_delay`).
2. **Gõ email khác hoa thường hoặc thừa khoảng trắng**, ví dụ `" An@Mail.com "` và `"an@mail.com"`. Kết quả mong đợi: cả hai là cùng một tài khoản, khi đăng ký lần hai thì báo trùng. Test ở Task 3 và Task 6.
3. **Không dò được email nào đã có tài khoản qua đăng nhập hoặc quên mật khẩu.** Đăng nhập bằng email không tồn tại phải trả đúng lỗi giống như sai mật khẩu. Yêu cầu đặt lại cho email lạ vẫn trả 204. Test ở Task 6 và Task 9.
4. **Dữ liệu quá lớn hoặc có ký tự Unicode.** Mật khẩu 1 MB bị chặn trước khi băm (413 hoặc 422). Mật khẩu có dấu tiếng Việt hoặc emoji được đếm theo ký tự. Test ở Task 2 và Task 4.
5. **Access token hết hạn khi đang dùng app.** App tự refresh, người dùng không bị đá ra. Nếu refresh token cũng hết hạn thì quay về trang đăng nhập đúng một lần, không lặp vô hạn. Test ở Task 12 và Task 13.

---

## Phiên bản và phụ thuộc

Researcher đã tra các phiên bản này ngày 2026-11-25. **Task 1 và Task 11 có bước kiểm tra lại** bằng `cargo search` / `cargo info` / `npm view`, rồi mới ghim chính xác. Phiên bản cuối cùng được ghi vào `.claude/CLAUDE.md` §2.4.

**Phương án dự phòng chung:** nếu API của một phiên bản lớn mới (jsonwebtoken 12, argon2 0.6, validator 0.21, utoipa 6, sqlx 0.9, TypeScript 7) khác với mô tả trong kế hoạch và không sửa được trong phạm vi task đó, thì ghim phiên bản lớn ổn định liền trước. Ghi lý do vào `.claude/CLAUDE.md` §2.4 và báo chủ dự án. Không đổi sang thư viện khác khi chưa được duyệt.

| Thành phần | Phiên bản dự kiến | Cần kiểm tra khi dựng khung |
|---|---|---|
| Rust toolchain | 1.100 (sqlx 0.9 cần ≥ 1.94) | `rustc --version` |
| axum / tokio / tower | 0.8.9 / 1.54 / 0.5 | đường dẫn kiểu `/{id}` |
| tower-http | 0.7.1 | có tương thích axum 0.8.9 không (`cargo tree -d` không được có hai bản `http` hoặc `tower`) |
| axum-extra (feature `cookie`) | 0.12.6 | có đúng bản dành cho axum 0.8 không |
| sqlx (`runtime-tokio`, `postgres`, `uuid`, `chrono`, `migrate`, `macros`) | 0.9.1 | thay đổi kiểu `SqlSafeStr` |
| jsonwebtoken | 12.0.0 | chọn feature crypto backend trên docs.rs (bản 12 là major mới) |
| argon2 | 0.6.0 | API của `PasswordHash` / `PasswordVerifier` |
| validator (`derive`) | 0.21.0 | cú pháp derive |
| utoipa (`chrono`, `uuid`) | 6.0.0 | |
| lettre (`tokio1`, `smtp-transport`, `builder`) | 0.11.24 | |
| serde, serde_json, thiserror, anyhow, tracing, tracing-subscriber (`env-filter`), uuid (`serde`), chrono (`serde`), dotenvy, rand, sha2 | theo bản stable mới nhất | |
| PostgreSQL image | `postgres:18.7` (hoặc bản vá 18.x mới nhất) | `uuidv7()` chạy được |
| Mailpit image | `axllent/mailpit:v1.34.0` | |
| Node.js | 24 LTS | |
| react / react-dom, vite, @vitejs/plugin-react | 19.3 / 8.3 / mới nhất | |
| typescript | 7.0.2 | **typescript-eslint có hỗ trợ TS 7 chưa.** Nếu chưa, dùng bản TS mới nhất mà typescript-eslint hỗ trợ và ghi lý do vào CLAUDE.md |
| react-router, @tanstack/react-query | 8.5.3, 5.108 | |
| vitest, @testing-library/react, eslint, prettier | 5.1, 16.3, 10.15, 3.10 | |

---

## Bản đồ file

```text
dateNaow/
├── .gitattributes                       # eol=lf cho file shell/Docker
├── docker-compose.yml                   # postgres, mailpit, backend
├── README.md                            # cách chạy dự án trên máy (Task 15)
├── backend/
│   ├── Cargo.toml, Cargo.lock, .env.example, .gitignore, Dockerfile, .sqlx/
│   ├── migrations/
│   │   ├── 20261125000001_create_users.sql
│   │   ├── 20261125000002_create_refresh_tokens.sql
│   │   ├── 20261125000003_create_auth_attempts.sql
│   │   └── 20261125000004_create_password_reset_tokens.sql
│   ├── src/
│   │   ├── main.rs                      # khởi động, tracing, migrate, serve, graceful shutdown
│   │   ├── lib.rs                       # pub mod ...; pub use routes::build_router; pub use state::AppState
│   │   ├── routes.rs                    # build_router: /api/v1 + middleware
│   │   ├── state.rs                     # AppState: ghép các dependency
│   │   ├── bin/make_admin.rs
│   │   ├── config/mod.rs                # Config::from_env
│   │   ├── shared/
│   │   │   ├── mod.rs, clock.rs, error.rs, extract.rs, actor.rs, client_ip.rs, openapi.rs, health.rs
│   │   ├── infrastructure/
│   │   │   ├── mod.rs, db.rs, telemetry.rs, email.rs
│   │   └── modules/
│   │       ├── mod.rs
│   │       ├── users/
│   │       │   ├── mod.rs
│   │       │   ├── domain/{mod.rs, entities/user.rs, value_objects/email.rs, enums/gender.rs, repositories/user_repository.rs}
│   │       │   ├── application/{mod.rs, get_current_user/{mod.rs,query.rs,handler.rs}, update_gender/{mod.rs,command.rs,handler.rs}}
│   │       │   ├── infrastructure/{mod.rs, repositories/{mod.rs, pg_user_repository.rs}}
│   │       │   └── presentation/{mod.rs, routes.rs, handlers.rs, dto.rs}
│   │       ├── auth/
│   │       │   ├── mod.rs
│   │       │   ├── domain/{mod.rs, value_objects/{password.rs, opaque_token.rs}, repositories/{refresh_token_repository.rs, attempt_repository.rs, password_reset_repository.rs}}
│   │       │   ├── application/{mod.rs, error.rs, ports/email_sender.rs,
│   │       │   │     services/{jwt.rs, password_hasher.rs, session.rs, attempt_limiter.rs},
│   │       │   │     register/{mod.rs,command.rs,handler.rs}, login/{...},
│   │       │   │     request_password_reset/{...}, confirm_password_reset/{...}}
│   │       │   ├── infrastructure/{mod.rs, repositories/{pg_refresh_token_repository.rs, pg_attempt_repository.rs, pg_password_reset_repository.rs}}
│   │       │   └── presentation/{mod.rs, routes.rs, handlers.rs, dto.rs, cookies.rs, extractors.rs}
│   │       └── admin/
│   │           ├── mod.rs
│   │           └── presentation/{mod.rs, extractors.rs}
│   └── tests/
│       ├── common/mod.rs                # TestApp, FakeClock, FakeMailer, test_config
│       └── integration/
│           ├── main.rs                  # #[path = "../common/mod.rs"] mod common; mod health; ...
│           ├── health.rs, errors.rs, users_repository.rs, auth_register_login.rs,
│           ├── auth_refresh_logout.rs, me_and_admin.rs, password_reset.rs, openapi.rs
└── frontend/
    ├── package.json, package-lock.json, .gitignore, .nvmrc, index.html
    ├── vite.config.ts, tsconfig*.json, eslint.config.js, .prettierrc.json
    └── src/
        ├── main.tsx
        ├── test/{setup.ts, renderApp.tsx}
        ├── app/{router.tsx, providers.tsx, NotFoundPage.tsx, layout/AppLayout.tsx, layout/AppLayout.module.css,
        │        guards/{RequireAuth.tsx, RequireAdmin.tsx}}
        ├── shared/
        │   ├── api/{client.ts, tokenStore.ts, errors.ts, types.ts}
        │   ├── ui/{Button.tsx, TextField.tsx, FormMessage.tsx, *.module.css}
        │   └── styles/global.css
        └── features/
            ├── auth/{api.ts, AuthProvider.tsx, messages.ts, pages/{SignInPage, SignUpPage, ForgotPasswordPage, ResetPasswordPage}.tsx}
            ├── home/HomePage.tsx
            ├── settings/{api.ts, SettingsPage.tsx}
            └── admin/AdminHomePage.tsx
```

Test unit của value object và service thuần nằm ngay trong file đó, trong khối `#[cfg(test)] mod tests` (CLAUDE.md §4.2). Test API và repository dùng PostgreSQL thật qua `#[sqlx::test]`: mỗi test có một database riêng và tự chạy migration. Test frontend đặt cạnh file được test, đuôi `*.test.ts(x)`.

---

## Task 0: Chuẩn bị (không viết code)

- [x] **Step 1:** (Đã làm 2026-11-25: chủ dự án đồng ý, đã commit.) Hỏi chủ dự án có muốn commit 3 file đang chưa commit (`.claude/CLAUDE.md`, spec, `docs/DECISIONS.md`) cùng hai file kế hoạch trên `main` trước khi tạo nhánh không. Chỉ commit khi được đồng ý.
- [x] **Step 2:** (Đã làm 2026-11-25: DECISIONS #9.) Ghi các đề xuất P1-P13 đã được duyệt vào `docs/DECISIONS.md` (mục #9 trở đi, ghi ngày duyệt và lý do), kèm các mục bị chủ dự án sửa hoặc từ chối.
- [ ] **Step 3:** `git switch -c feat/plan-1-foundation-accounts`
- [ ] **Step 4:** Kiểm tra công cụ: `rustc --version` (≥ 1.94), `node --version` (24.x), `docker --version`, `docker compose version`. Thiếu công cụ nào thì báo chủ dự án, không tự cài.
- [ ] **Step 5:** Khi chủ dự án đồng ý (P13), cài `sqlx-cli` đúng phiên bản sqlx đã chọn và chỉ bật feature postgres: `cargo install sqlx-cli --version <x.y.z> --no-default-features --features rustls,postgres --locked`. Nếu `--locked` báo lỗi vì gói không có `Cargo.lock` thì chạy lại không có `--locked`. Researcher cho biết sqlx 0.9 có thể không còn kèm lockfile, điều này chưa được kiểm chứng.

---

## Task 1: Docker Compose, khung backend và `/api/v1/health`

**Files:**
- Create: `.gitattributes`, `docker-compose.yml`, `backend/Cargo.toml`, `backend/.env.example`, `backend/.gitignore`
- Create: `backend/src/{main.rs, lib.rs, routes.rs, state.rs}`, `backend/src/config/mod.rs`, `backend/src/shared/{mod.rs, clock.rs, health.rs}`, `backend/src/infrastructure/{mod.rs, db.rs, telemetry.rs}`
- Test: `backend/tests/common/mod.rs`, `backend/tests/integration/{main.rs, health.rs}`

**Interfaces:**
- Produces:
  - `config::Config { database_url: String, bind_addr: SocketAddr, app_base_url: String }` và `Config::from_env() -> anyhow::Result<Config>`. Các task sau thêm trường. `Debug` tự viết tay và che mọi trường bí mật bằng `"***"`, kể cả `database_url` vì chuỗi này chứa mật khẩu DB.
  - `shared::clock::{Clock, SystemClock}`: `pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }`
  - `state::AppState` (`Clone`), có các trường `pub db: PgPool`, `pub config: Arc<Config>`, `pub clock: Arc<dyn Clock>`. Hàm tạo `AppState::new(config: Config, db: PgPool, clock: Arc<dyn Clock>) -> AppState`. Task 9 thêm tham số `mailer`.
  - `routes::build_router(state: AppState) -> axum::Router`
  - `GET /api/v1/health`: trả 200 `{"status":"ok","database":"ok"}`. Nếu `SELECT 1` lỗi thì trả 503 `{"status":"degraded","database":"unavailable"}`.
  - Test helper `common::TestApp::new(pool: PgPool) -> TestApp`, có trường `pub clock: Arc<FakeClock>`. Các hàm `get/post/patch(path) -> TestRequest`. `TestRequest` có `.json(&serde_json::Value)`, `.bearer(&str)`, `.cookie(&str)`, `.header(&str, &str)`, và `.send().await -> TestResponse { status: StatusCode, headers: HeaderMap, body: serde_json::Value, text: String }`. `TestResponse` có `set_cookie(name) -> Option<String>` trả cả header và `cookie_value(name) -> Option<String>`.
  - `FakeClock::new(start: DateTime<Utc>)` và `advance(&self, chrono::Duration)`. Thời điểm bắt đầu mặc định là `2026-11-25T09:00:00Z`.
  - `common::test_config() -> Config`
  - Router trong test được bọc `MockConnectInfo(SocketAddr::from(([203,0,113,7], 40000)))` và gọi bằng `tower::ServiceExt::oneshot`.

- [ ] **Step 1: Viết `docker-compose.yml`**
  - Service `postgres`: image `postgres:18.x`, port 5432, volume có tên, healthcheck `pg_isready`.
  - Service `mailpit`: image `axllent/mailpit:v1.34.0`, port 1025 (SMTP) và 8025 (giao diện web).
  - Service `backend` thêm ở Task 10.
  - `.env.example` có: `DATABASE_URL=postgres://datenaow:datenaow@localhost:5432/datenaow`, `BIND_ADDR=0.0.0.0:8080`, `APP_BASE_URL=http://localhost:5173`, `RUST_LOG=info,datenaow=debug`.
- [ ] **Step 2: Tạo `Cargo.toml`** (tên package `datenaow`, edition 2024). Kiểm tra từng phiên bản trong bảng "Phiên bản và phụ thuộc", rồi thêm bằng `cargo add <crate>@=<x.y.z>`. Chạy `cargo tree -d` để chắc không có hai bản `axum`, `http` hoặc `tower`.
- [ ] **Step 3: Viết test chạy trước, cho thấy lỗi** trong `tests/integration/health.rs`:

```rust
#[sqlx::test(migrations = false)]
async fn health_returns_ok_when_database_reachable(pool: PgPool) {
    let app = TestApp::new(pool);
    let res = app.get("/api/v1/health").send().await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, json!({"status": "ok", "database": "ok"}));
}

#[sqlx::test(migrations = false)]
async fn every_response_has_request_id_header(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/health").send().await;
    assert!(res.headers.contains_key("x-request-id"));
}
```

- [ ] **Step 4:** `docker compose up -d postgres mailpit`. Chép `.env.example` thành `.env`, rồi chạy `cargo test --test integration health`. Kết quả mong đợi: báo lỗi biên dịch vì chưa có `TestApp` và `build_router`.
- [ ] **Step 5: Viết phần thân.**
  - `build_router` đặt `/api/v1/health` dưới `/api/v1`.
  - Gom middleware vào **một** `tower::ServiceBuilder`, vì nếu gọi `.layer()` nhiều lần thì layer thêm sau lại chạy trước. Thứ tự từ ngoài vào trong:
    1. `SetRequestIdLayer::x_request_id(MakeRequestUuid)`
    2. `TraceLayer::new_for_http().make_span_with(...)`: span ghi trường `request_id` lấy từ header `x-request-id`, cùng `method` và `uri`
    3. `PropagateRequestIdLayer::x_request_id()`
    4. `HandleErrorLayer`: đổi lỗi `tower::timeout::error::Elapsed` thành `ApiError::Timeout` (408, Task 2). Ở Task 1 tạm trả `StatusCode::REQUEST_TIMEOUT`.
    5. `tower::timeout::TimeoutLayer` 15 giây (P9)
  - Giới hạn body dùng `axum::extract::DefaultBodyLimit::max(65_536)` (P9), không dùng `RequestBodyLimitLayer` của tower-http. Layer đó tự trả 413 không phải JSON mỗi khi `Content-Length` vượt giới hạn, mà trình duyệt luôn gửi header này.
  - `main.rs` lần lượt: `dotenvy::dotenv().ok()`, `telemetry::init()` (tracing-subscriber với `EnvFilter`), `db::connect(&config.database_url)` (`PgPoolOptions`, tối đa 10 kết nối, timeout lấy kết nối 5 giây), rồi `axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())` kèm `with_graceful_shutdown`, chờ Ctrl+C hoặc SIGTERM trên Unix.
  - **Chưa gọi `sqlx::migrate!()`**, vì macro này báo lỗi biên dịch khi chưa có thư mục `migrations/`. Task 3 thêm lời gọi cùng migration đầu tiên.
- [ ] **Step 6:** Chạy `cargo test --test integration health`. Kết quả mong đợi: PASS (2 test).
- [ ] **Step 7:** Chạy `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings`. Kết quả mong đợi: không có cảnh báo. Ghi phiên bản đã ghim vào `.claude/CLAUDE.md` §2.4.
- [ ] **Step 8: Commit** (khi được đồng ý): `git add .gitattributes docker-compose.yml backend .claude/CLAUDE.md && git commit -m "feat(backend): scaffold axum service with health check and docker compose"`

---

## Task 2: Mô hình lỗi, JSON được kiểm tra và OpenAPI

**Files:**
- Create: `backend/src/shared/{error.rs, extract.rs, openapi.rs}`
- Modify: `backend/src/routes.rs`, `backend/src/shared/health.rs` (thêm `#[utoipa::path]`)
- Test: `backend/tests/integration/{errors.rs, openapi.rs}`, unit test trong `error.rs`

**Interfaces:**
- Produces:
  - `pub type FieldErrors = BTreeMap<String, Vec<String>>`, ánh xạ tên trường sang danh sách mã lỗi, ví dụ `"password" → ["too_short"]`.
  - Kiểu lỗi chung:

    ```rust
    pub enum ApiError {
        BadRequest(&'static str),
        Validation(FieldErrors),
        Unauthorized(&'static str),
        Forbidden,
        NotFound,
        Conflict(&'static str),
        PayloadTooLarge,
        Timeout,
        RateLimited { retry_after_secs: u64 },
        Internal(anyhow::Error),
    }
    ```

  - `impl IntoResponse for ApiError` tạo body theo P2. Mã lỗi mặc định: `bad_request` / `invalid_json`, `validation_error`, `unauthorized` (hoặc mã truyền vào), `forbidden`, `not_found`, `payload_too_large`, `request_timeout` (408), `rate_limited`, `internal_error`. Riêng `Internal` thì ghi `tracing::error!(error = ?e)` và trả câu `"Đã có lỗi, bạn thử lại sau nhé."`.
  - `shared::extract::ValidatedJson<T>(pub T)` với `T: DeserializeOwned + Validate`. Ánh xạ `JsonRejection` như sau:
    - `JsonSyntaxError`, `JsonDataError` (sai kiểu, thiếu trường bắt buộc, trường lạ khi có `deny_unknown_fields`) và `MissingJsonContentType` đều trả `ApiError::BadRequest("invalid_json")`. Mặc định axum trả 422 cho `JsonDataError`, ở đây cố ý đổi thành 400. Như vậy 422 chỉ dành cho lỗi luật, kèm `fields`.
    - `BytesRejection` có status 413 (vượt `DefaultBodyLimit`) trả `ApiError::PayloadTooLarge`.
    - Validate thất bại trả `ApiError::Validation`, với mã lỗi lấy từ `ValidationError.code`.
  - `shared::openapi::ApiDoc` (`#[derive(OpenApi)]`) và route `GET /api/v1/openapi.json`. Mỗi task thêm endpoint sau này đều thêm đường dẫn của nó vào `ApiDoc`.

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi.**
  - Unit test trong `error.rs`: `validation_maps_to_422_with_fields`, `rate_limited_sets_retry_after_header`, `internal_hides_details` (body không chứa thông điệp lỗi gốc), `unauthorized_uses_given_code`.
  - Test tích hợp trong `errors.rs`. Test tự dựng một router riêng có route `POST /probe` nhận `ValidatedJson<Probe>`, với `Probe { #[validate(length(min = 3))] name: String }`:

```rust
#[tokio::test] async fn malformed_json_returns_400_invalid_json() { /* body "{" */ assert_eq!(res.body["error"]["code"], "invalid_json"); }
#[tokio::test] async fn failed_validation_returns_422_with_field_codes() { assert_eq!(res.body["error"]["fields"]["name"][0], "length"); }
#[tokio::test] async fn body_over_64_kib_with_content_length_returns_413_json() { /* gửi kèm header Content-Length như trình duyệt */ assert_eq!(res.status, 413); assert_eq!(res.body["error"]["code"], "payload_too_large"); }
#[tokio::test] async fn wrong_field_type_returns_400_invalid_json() { /* {"name": 5} */ assert_eq!(res.body["error"]["code"], "invalid_json"); }
#[tokio::test] async fn slow_handler_returns_408_json() { /* route test ngủ lâu hơn timeout; test dựng router với timeout 50ms */ assert_eq!(res.body["error"]["code"], "request_timeout"); }
#[sqlx::test(migrations = false)] async fn openapi_json_is_served(pool: PgPool) { assert!(res.body["paths"]["/api/v1/health"].is_object()); }
```

- [ ] **Step 2:** Chạy `cargo test`. Kết quả mong đợi: FAIL, vì chưa có `ApiError` và `ValidatedJson`.
- [ ] **Step 3:** Viết `ApiError`, `ValidatedJson` và `ApiDoc` theo các chữ ký trên. Đổi `HandleErrorLayer` của Task 1 sang trả `ApiError::Timeout`. Thời gian timeout lấy từ trường mới `config.request_timeout: std::time::Duration` (mặc định 15 giây, biến môi trường `REQUEST_TIMEOUT_SECS`). Test timeout tạo `TestApp` với config đặt 50ms.
- [ ] **Step 4:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 5: Commit:** `feat(backend): add api error envelope, validated json extractor and openapi doc`

---

## Task 3: Bảng `users`, domain `users` và `PgUserRepository`

**Files:**
- Create: `backend/migrations/20261125000001_create_users.sql`
- Create: `backend/src/shared/actor.rs`, toàn bộ `backend/src/modules/users/{domain, infrastructure}` theo bản đồ file, `backend/src/modules/mod.rs`
- Test: unit test trong `email.rs` và `gender.rs`, `backend/tests/integration/users_repository.rs`

**Interfaces:**
- Produces:
  - `shared::actor::Role { User, Admin }`, có `as_str() -> &'static str`, `FromStr`, và `Serialize`/`Deserialize` với `rename_all = "snake_case"` (vì nằm trong JWT claims). `Role` nằm ở `shared` vì mọi module đều cần để kiểm tra quyền.
  - `shared::actor::Actor { pub user_id: Uuid, pub role: Role }`, có `fn require_admin(&self) -> Result<(), Forbidden>`. `Forbidden` là struct lỗi rỗng, `impl From<Forbidden> for ApiError`.
  - `users::domain::Email`. `Email::parse(&str) -> Result<Email, EmailError>` cắt khoảng trắng hai đầu, đổi sang chữ thường, tối đa 254 ký tự và kiểm tra định dạng bằng `validator::ValidateEmail`. Có `as_str() -> &str`. `EmailError::{Empty, TooLong, Invalid}`, ứng với các mã `"required"`, `"too_long"`, `"invalid_email"`.
  - `users::domain::Gender { Male, Female, Other, PreferNotToSay }`. Serde dùng `snake_case`, gồm `male`, `female`, `other`, `prefer_not_to_say`. Có `as_str()` và `FromStr`.
  - `users::domain::User { id: Uuid, email: Email, role: Role, gender: Option<Gender>, created_at: DateTime<Utc> }`. Struct này **không chứa mã băm mật khẩu**.
  - `users::domain::UserCredentials { user_id: Uuid, password_hash: String, role: Role }`
  - `users::domain::NewUser { email: Email, password_hash: String }`
  - `UserRepoError::{EmailTaken, NotFound, Database(Box<dyn std::error::Error + Send + Sync>)}`
  - Trait repository:

    ```rust
    pub trait UserRepository: Send + Sync + 'static {
        fn create(&self, new_user: NewUser) -> impl Future<Output = Result<User, UserRepoError>> + Send;
        fn find_by_id(&self, id: Uuid) -> impl Future<Output = Result<Option<User>, UserRepoError>> + Send;
        fn find_credentials_by_email(&self, email: &Email) -> impl Future<Output = Result<Option<UserCredentials>, UserRepoError>> + Send;
        fn update_gender(&self, id: Uuid, gender: Option<Gender>) -> impl Future<Output = Result<User, UserRepoError>> + Send;
        fn set_role(&self, email: &Email, role: Role) -> impl Future<Output = Result<bool, UserRepoError>> + Send;
    }
    ```

  - `users::infrastructure::PgUserRepository::new(pool: PgPool)`, `Clone`. Dùng `UserRow` riêng của tầng DB rồi map sang `User`. Lỗi vi phạm unique `uq_users_email` được đổi thành `EmailTaken`.
  - Hàm `pub async fn set_password_hash_in_tx(conn: &mut PgConnection, user_id: Uuid, hash: &str) -> Result<(), UserRepoError>` (P11). Đây là hàm của tầng hạ tầng, không nằm trong trait. Task 9 gọi hàm này bên trong transaction của `auth`.
  - `main.rs` thêm `sqlx::migrate!().run(&pool).await?` trước khi serve.

- [ ] **Step 1: Viết migration.**

```sql
CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT uuidv7(),
    email         TEXT NOT NULL CHECK (length(email) <= 254), -- chuẩn hóa chữ thường do Email::parse đảm nhận; không CHECK lower() vì Rust và Postgres có thể đổi chữ hoa ngoài ASCII khác nhau
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    gender        TEXT NULL CHECK (gender IN ('male', 'female', 'other', 'prefer_not_to_say')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX uq_users_email ON users (email);
```

- [ ] **Step 2: Viết test chạy trước, cho thấy lỗi.**
  - Unit test của `Email`: `parse_trims_and_lowercases` (`"  An@Example.COM "` thành `"an@example.com"`), `rejects_empty`, `rejects_missing_at`, `rejects_longer_than_254_chars`.
  - Unit test của `Gender`: `parses_all_four_values`, `rejects_unknown_value`.
  - Unit test của `Actor`: `require_admin_ok_for_admin`, `require_admin_forbidden_for_user`.
  - Test repository (`#[sqlx::test]`):

```rust
async fn create_then_find_by_id_returns_same_user(pool: PgPool) { assert_eq!(found.email.as_str(), "an@example.com"); assert_eq!(found.role, Role::User); assert_eq!(found.gender, None); }
async fn create_with_existing_email_returns_email_taken(pool: PgPool) { assert!(matches!(err, UserRepoError::EmailTaken)); }
async fn find_credentials_by_email_returns_hash_and_role(pool: PgPool) { assert_eq!(creds.password_hash, "hash-1"); }
async fn find_credentials_for_unknown_email_returns_none(pool: PgPool) { assert!(res.is_none()); }
async fn update_gender_sets_then_clears(pool: PgPool) { assert_eq!(u1.gender, Some(Gender::Female)); assert_eq!(u2.gender, None); }
async fn update_gender_for_missing_user_returns_not_found(pool: PgPool) { /* ... */ }
async fn set_role_promotes_existing_user_and_reports_missing(pool: PgPool) { assert!(promoted); assert!(!missing); }
async fn set_password_hash_in_tx_is_rolled_back_with_transaction(pool: PgPool) { /* begin → set → rollback → hash cũ còn nguyên */ }
```

- [ ] **Step 3:** Chạy `cargo test`. Kết quả mong đợi: FAIL.
- [ ] **Step 4:** Viết phần domain và `PgUserRepository` bằng `sqlx::query_as!`. `update_gender` cập nhật luôn `updated_at = now()`.
- [ ] **Step 5:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 6: Commit:** `feat(users): add users table, domain types and postgres repository`

---

## Task 4: Chính sách mật khẩu, Argon2 và JWT

**Files:**
- Create: `backend/src/modules/auth/{mod.rs, domain/mod.rs, domain/value_objects/password.rs, application/mod.rs, application/services/{jwt.rs, password_hasher.rs}}`
- Modify: `backend/src/config/mod.rs`: thêm `jwt_secret: String` (≥ 32 byte, nếu ngắn hơn thì lỗi lúc khởi động), `access_token_ttl_secs: i64` (mặc định 900), `argon2_memory_kib: u32`, `argon2_iterations: u32`, `argon2_parallelism: u32` (mặc định lấy theo `argon2::Params::default()`). Cập nhật `.env.example` và `test_config()` (khi test dùng `argon2_memory_kib = 1024`, `argon2_iterations = 1`).
- Test: unit test trong từng file

**Interfaces:**
- Produces:
  - `auth::domain::PlainPassword`. `PlainPassword::parse(String) -> Result<PlainPassword, PasswordError>` đếm `chars().count()`, chỉ nhận từ 8 đến 128. `PasswordError::{TooShort, TooLong}` ứng với mã `"too_short"`, `"too_long"`. `Debug` in ra `PlainPassword(***)`. Có `expose(&self) -> &str`.
  - `auth::application::services::PasswordHashService::new(params: argon2::Params)`. Có hai hàm `async fn hash(&self, p: &PlainPassword) -> Result<String, HashError>` và `async fn verify(&self, p: &PlainPassword, phc_hash: &str) -> Result<bool, HashError>`. Cả hai chạy trong `tokio::task::spawn_blocking`. Thêm `async fn verify_dummy(&self, p: &PlainPassword)`: băm thử với một mã băm giả tạo sẵn lúc khởi tạo, để đăng nhập bằng email lạ tốn thời gian ngang với email có thật.
  - `auth::application::services::JwtService::new(secret: &[u8], access_ttl: chrono::Duration)`, có:
    - `issue(&self, user_id: Uuid, role: Role, now: DateTime<Utc>) -> Result<String, TokenError>`
    - `verify(&self, token: &str, now: DateTime<Utc>) -> Result<AccessClaims, TokenError>`
    - `access_ttl_secs(&self) -> i64`
  - `AccessClaims { sub: Uuid, role: Role, iat: i64, exp: i64 }`. `TokenError::{Invalid, Expired, Encode}`.
  - Chỉ dùng thuật toán HS256. Tắt việc kiểm tra `exp` theo giờ hệ thống của jsonwebtoken (`validate_exp = false`) và tự so `now >= exp` thì trả `Expired`, để test dùng được `FakeClock`.

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi.**

```rust
// password.rs
fn rejects_7_chars() / fn accepts_8_chars() / fn accepts_128_chars() / fn rejects_129_chars()
fn counts_unicode_chars_not_bytes() { assert!(PlainPassword::parse("mậtkhẩu😺".into()).is_ok()); } // 8 ký tự, nhiều hơn 8 byte
fn debug_never_prints_password() { assert_eq!(format!("{:?}", p), "PlainPassword(***)"); }
// password_hasher.rs (#[tokio::test], dùng tham số nhỏ)
async fn hash_then_verify_returns_true() / async fn verify_wrong_password_returns_false()
async fn hash_is_argon2id_phc_string() { assert!(h.starts_with("$argon2id$")); }
// jwt.rs
fn issue_then_verify_returns_same_claims() { assert_eq!(c.sub, id); assert_eq!(c.role, Role::User); assert_eq!(c.exp - c.iat, 900); }
fn token_is_expired_at_exp() { assert!(matches!(svc.verify(&t, now + Duration::seconds(900)), Err(TokenError::Expired))); }
fn tampered_payload_is_rejected() / fn token_signed_with_other_secret_is_rejected() / fn hs512_token_is_rejected()
```

- [ ] **Step 2:** Chạy `cargo test`. Kết quả mong đợi: FAIL.
- [ ] **Step 3:** Viết code theo các chữ ký trên. Trước khi viết, đọc docs.rs của jsonwebtoken 12 để chọn feature crypto backend.
- [ ] **Step 4:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 5: Commit:** `feat(auth): add password policy, argon2 hasher and jwt service`

---

## Task 5: Refresh token: lưu trữ và `SessionService`

**Files:**
- Create: `backend/migrations/20261125000002_create_refresh_tokens.sql`
- Create: `backend/src/modules/auth/domain/value_objects/opaque_token.rs`, `domain/repositories/refresh_token_repository.rs`, `infrastructure/repositories/pg_refresh_token_repository.rs`, `application/services/session.rs`, `application/error.rs`
- Modify: config thêm `refresh_token_ttl_days: i64` (mặc định 30)
- Test: unit test trong `opaque_token.rs`, `backend/tests/integration/auth_refresh_logout.rs` (phần repository)

**Interfaces:**
- Consumes: `UserRepository`, `JwtService`, `Clock` (Task 1, 3, 4)
- Produces:
  - `OpaqueToken`:
    - `generate() -> OpaqueToken`: 32 byte từ `rand` (nguồn ngẫu nhiên của hệ điều hành), mã hóa hex thành 64 ký tự thường.
    - `from_client(s: &str) -> Result<OpaqueToken, InvalidToken>`: chỉ nhận đúng 64 ký tự hex thường.
    - `as_str() -> &str`.
    - `hash(&self) -> [u8; 32]`: SHA-256 của chuỗi.
    - `Debug` in ra `OpaqueToken(***)`.
  - `RefreshTokenRecord { id: Uuid, user_id: Uuid, expires_at: DateTime<Utc>, revoked_at: Option<DateTime<Utc>>, replaced_at: Option<DateTime<Utc>> }`
  - `RefreshRepoError::{AlreadyRevoked, Database(..)}`
  - Trait repository, các hàm đều trả `impl Future<Output = Result<_, RefreshRepoError>> + Send`:

    ```rust
    pub trait RefreshTokenRepository: Send + Sync + 'static {
        fn insert(&self, user_id: Uuid, token_hash: [u8; 32], expires_at: DateTime<Utc>) -> /* ... */;
        fn find_by_hash(&self, token_hash: [u8; 32]) -> /* Option<RefreshTokenRecord> */;
        // một transaction: UPDATE ... SET revoked_at = now, replaced_at = now
        //                  WHERE id = old_id AND revoked_at IS NULL;
        // 0 dòng được sửa → AlreadyRevoked; ngược lại INSERT token mới
        fn rotate(&self, old_id: Uuid, user_id: Uuid, new_hash: [u8; 32], new_expires_at: DateTime<Utc>, now: DateTime<Utc>) -> /* ... */;
        fn revoke(&self, id: Uuid, now: DateTime<Utc>) -> /* ... */;
        fn revoke_all_for_user(&self, user_id: Uuid, now: DateTime<Utc>) -> /* ... */;
    }
    ```

  - Kiểu lỗi của module:

    ```rust
    pub enum AuthError {
        Validation(FieldErrors),
        EmailTaken,
        InvalidCredentials,
        RateLimited { retry_after_secs: u64 },
        InvalidRefreshToken,
        RefreshConflict,
        InvalidOrExpiredResetToken,
        Internal(Box<dyn std::error::Error + Send + Sync>),
    }
    ```

    `impl From<AuthError> for ApiError` ánh xạ lần lượt sang: 422; 409 `email_taken`; 401 `invalid_credentials`; 429; 401 `invalid_refresh_token`; 409 `refresh_conflict`; 400 `invalid_or_expired_token`; 500.
  - `Session { access_token: String, access_expires_in_secs: i64, refresh_token: OpaqueToken, refresh_expires_at: DateTime<Utc> }`
  - `SessionService<U: UserRepository, R: RefreshTokenRepository>::new(users: U, tokens: R, jwt: Arc<JwtService>, clock: Arc<dyn Clock>, refresh_ttl: chrono::Duration)`. Task này chỉ viết `async fn start(&self, user: &User) -> Result<Session, AuthError>`. Token mới hết hạn sau `now + refresh_ttl`. Task 7 viết thêm `refresh` và `end`, kèm test.

- [ ] **Step 1: Viết migration.** Bảng `refresh_tokens (id UUID PK DEFAULT uuidv7(), user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, token_hash BYTEA NOT NULL CHECK (length(token_hash) = 32), expires_at TIMESTAMPTZ NOT NULL, revoked_at TIMESTAMPTZ NULL, replaced_at TIMESTAMPTZ NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now())`. Thêm `uq_refresh_tokens_token_hash` và `idx_refresh_tokens_user_id`.
- [ ] **Step 2: Viết test chạy trước, cho thấy lỗi.**
  - Unit test của `OpaqueToken`: `generate_is_64_lowercase_hex`, `two_generated_tokens_differ`, `from_client_rejects_63_chars_and_uppercase`, `hash_is_stable_for_same_token`.
  - Test repository: `rotate_marks_old_revoked_and_inserts_new`, `rotate_already_revoked_returns_already_revoked`, `revoke_all_for_user_revokes_every_active_token_only_for_that_user`, `revoke_sets_revoked_at_but_leaves_replaced_at_null`.
  - Test service (`#[sqlx::test]`, gọi thẳng `SessionService` với repository Postgres thật và `FakeClock`): `start_returns_jwt_valid_for_900s_and_stores_only_hash` (cột `token_hash` khác chuỗi token gửi cho client, và bằng `OpaqueToken::hash()`), `start_sets_refresh_expiry_30_days_from_now`.
- [ ] **Step 3:** Chạy `cargo test`. Kết quả mong đợi: FAIL.
- [ ] **Step 4:** Viết `OpaqueToken`, `PgRefreshTokenRepository`, `AuthError` và `SessionService::start`.
- [ ] **Step 5:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 6: Commit:** `feat(auth): add refresh token storage and session service`

---

## Task 6: Đăng ký, đăng nhập và giới hạn số lần sai

**Files:**
- Create: `backend/migrations/20261125000003_create_auth_attempts.sql`
- Create: `auth/domain/repositories/attempt_repository.rs`, `auth/infrastructure/repositories/pg_attempt_repository.rs`, `auth/application/services/attempt_limiter.rs`, `auth/application/register/*`, `auth/application/login/*`, `auth/presentation/{mod.rs, routes.rs, handlers.rs, dto.rs, cookies.rs}`, `backend/src/shared/client_ip.rs`
- Modify: `state.rs` (ghép handler), `routes.rs` (gắn `/api/v1/auth`), `openapi.rs`, config thêm `cookie_secure: bool` (mặc định `true`, trong `.env.example` là `false`) và `trust_proxy: bool` (mặc định `false`)
- Test: `backend/tests/integration/auth_register_login.rs`. Thêm helper `TestApp::register(email, password) -> TestResponse`.

**Interfaces:**
- Consumes: `SessionService`, `PasswordHashService`, `UserRepository`, `Email`, `PlainPassword`
- Produces:
  - Trait `AttemptRepository: Send + Sync + 'static`, gồm:
    - `record(key_hash: [u8; 32], at: DateTime<Utc>) -> i64`: trả `id` của dòng vừa ghi
    - `count_since(key_hash: [u8; 32], since: DateTime<Utc>) -> i64`
    - `delete_by_id(id: i64)`
    - `clear(key_hash: [u8; 32])`
    - `delete_older_than(before: DateTime<Utc>)`
  - `LimitPolicy { max: i64, window: chrono::Duration }`. Bốn hằng số theo P5: `LOGIN_PER_EMAIL` (5 lần / 15 phút), `LOGIN_PER_IP` (20 / 15 phút), `RESET_PER_EMAIL` (3 / 60 phút), `RESET_PER_IP` (10 / 60 phút). *Theo #13 nay là sáu hằng số:* `LOGIN_PER_PAIR` (5 / 15), `LOGIN_PER_IP` (20 / 15), `LOGIN_PER_EMAIL` (50 / 15), `RESET_PER_PAIR` (3 / 60), `RESET_PER_IP` (10 / 60), `RESET_PER_EMAIL` (10 / 60); thêm `LimitKey::pair`, và `acquire` nhận các tầng `&[&[LimitKey]]` (tầng cặp + IP trước, tầng email chỉ khi tầng trước qua).
  - `LimitKey { label: String, policy: LimitPolicy }` với `label` có dạng `"login:email:an@example.com"`. Chỗ duy nhất băm `label` bằng SHA-256 là bên trong `AttemptLimiter`.
  - `AttemptLimiter<A: AttemptRepository>::new(repo: A, clock: Arc<dyn Clock>)`, có:
    - `async fn acquire(&self, keys: &[LimitKey]) -> Result<AttemptTicket, AuthError>` *(#13: nay nhận các tầng `&[&[LimitKey]]`)*: **ghi trước, đếm sau**.
      - Với mỗi khóa: gọi `record` (mỗi câu lệnh tự commit ngay), rồi `count_since(now - window)`. Số đếm có tính cả lần vừa ghi.
      - Có khóa nào đếm ra `> max` thì trả `RateLimited { retry_after_secs: policy.window.num_seconds() as u64 }`. Các dòng vừa ghi vẫn được giữ, vì đó cũng là một lần thử.
      - Cách này chặn được nhiều request gửi cùng lúc. Request thứ i commit dòng của nó trước khi đếm, nên đếm ra ít nhất i. Vì vậy chỉ có `max` request đầu tiên lọt qua.
      - Mỗi lần gọi cũng xóa các dòng cũ hơn 60 phút.
    - `AttemptTicket { ids: Vec<(LimitKey, i64)> }` giữ id của các dòng vừa ghi.
    - `async fn forgive(&self, ticket: &AttemptTicket, key: &LimitKey)`: xóa dòng vừa ghi của khóa đó, dùng khi lần thử thành công nên không bị tính.
    - `async fn clear(&self, key: &LimitKey)`: xóa mọi dòng của khóa.
  - Khóa IP: IPv4 dùng nguyên địa chỉ. IPv6 dùng dải /64, ví dụ `login:ip:2001:db8:1:2::/64` (P5).
  - `RegisterCommand { email: String, password: String }` và `RegisterHandler::execute(&self, cmd) -> Result<(User, Session), AuthError>`. Email và mật khẩu sai được gom lại trả trong một lần `Validation`.
  - `LoginCommand { email: String, password: String, client_ip: IpAddr }` và `LoginHandler::execute(&self, cmd) -> Result<(User, Session), AuthError>`. Thứ tự xử lý:
    1. Email hoặc mật khẩu sai định dạng thì trả `InvalidCredentials`, không ghi lần thử. Sai định dạng không giúp dò được tài khoản nào.
    2. `acquire` cho khóa email và khóa IP. Vượt giới hạn thì trả `RateLimited`, lúc này chưa kiểm tra mật khẩu. *(#13: tầng 1 là khóa cặp + khóa IP, tầng 2 là khóa email.)*
    3. Không tìm thấy tài khoản: gọi `verify_dummy`, trả `InvalidCredentials`. Lần thử vẫn được tính.
    4. Mật khẩu sai: trả `InvalidCredentials`. Lần thử vẫn được tính.
    5. Mật khẩu đúng: `clear` khóa email, `forgive` khóa IP, rồi `start` phiên. *(#13: `clear` cả khóa cặp của IP này; khóa cặp của IP khác giữ nguyên.)*
  - `shared::client_ip::ClientIp(pub IpAddr)` là extractor. Lấy từ `ConnectInfo<SocketAddr>`. Khi `config.trust_proxy` bật thì lấy địa chỉ IP hợp lệ ngoài cùng bên phải trong `X-Forwarded-For`.
  - DTO:
    - `RegisterRequest` / `LoginRequest { email: String, password: String }` có `#[serde(deny_unknown_fields)]`.
      - Hai trường không gắn `#[validate(length)]` riêng. Giới hạn 64 KiB của body đã chặn dữ liệu quá lớn.
      - Domain (`Email::parse`, `PlainPassword::parse`) trả các mã lỗi `too_short` / `too_long` / `invalid_email`, nên mã lỗi trùng với `messages.ts` ở frontend.
      - Thứ tự xử lý: `PlainPassword::parse` đếm ký tự trước khi băm, nên chuỗi 60 KiB bị từ chối ngay, không phải chạy Argon2.
    - `AuthResponse { access_token: String, token_type: "Bearer", expires_in: i64, user: UserResponse }`
    - `UserResponse { id: Uuid, email: String, role: "user" | "admin", gender: Option<"male" | "female" | "other" | "prefer_not_to_say"> }` nằm ở `users::presentation::dto` (task này tạo file đó), có `impl From<&User> for UserResponse`. `auth` dùng lại kiểu này.
  - `AppState` thêm `pub jwt: Arc<JwtService>`, `pub users_repo: PgUserRepository` và `pub auth: AuthHandlers { register, login, sessions }`, mỗi handler bọc trong `Arc`.
  - `auth::presentation::cookies`:
    - `refresh_cookie(token: &OpaqueToken, max_age_secs: i64, secure: bool) -> Cookie<'static>`
    - `clear_refresh_cookie(secure: bool) -> Cookie<'static>`
    - `REFRESH_COOKIE = "dn_refresh"`
  - Endpoint:
    - `POST /api/v1/auth/register` trả 201 `AuthResponse` kèm `Set-Cookie`.
    - `POST /api/v1/auth/login` trả 200 `AuthResponse` kèm `Set-Cookie`.
    - Cả hai được thêm vào `ApiDoc`.
  - Router `/api/v1/auth` (và `/api/v1/me` ở Task 8) có layer `SetResponseHeaderLayer::overriding(CACHE_CONTROL, "no-store")` (P2).

- [ ] **Step 1: Viết migration.** Bảng `auth_attempts (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, key_hash BYTEA NOT NULL CHECK (length(key_hash) = 32), attempted_at TIMESTAMPTZ NOT NULL)` và `idx_auth_attempts_key_hash_attempted_at (key_hash, attempted_at)`.
- [ ] **Step 2: Viết test chạy trước, cho thấy lỗi** (`#[sqlx::test]`):

```rust
async fn register_returns_201_with_token_and_refresh_cookie(pool: PgPool) {
    assert_eq!(res.status, StatusCode::CREATED);
    assert_eq!(res.body["user"]["email"], "an@example.com");
    assert_eq!(res.body["user"]["role"], "user");
    assert!(res.body["user"]["gender"].is_null());
    assert_eq!(res.body["expires_in"], 900);
    let c = res.set_cookie("dn_refresh").unwrap();
    assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict") && c.contains("Path=/api/v1/auth") && c.contains("Max-Age=2592000"));
}
async fn register_normalises_email(pool: PgPool) { /* "  An@Example.COM " */ assert_eq!(res.body["user"]["email"], "an@example.com"); }
async fn register_same_email_different_case_returns_409_email_taken(pool: PgPool) { assert_eq!(res.body["error"]["code"], "email_taken"); }
async fn register_short_password_returns_422_with_password_field(pool: PgPool) { assert_eq!(res.body["error"]["fields"]["password"][0], "too_short"); }
async fn register_password_of_129_chars_returns_422_too_long(pool: PgPool) { assert_eq!(res.body["error"]["fields"]["password"][0], "too_long"); }
async fn auth_responses_have_cache_control_no_store(pool: PgPool) { assert_eq!(res.headers["cache-control"], "no-store"); }
async fn register_response_never_contains_password_hash(pool: PgPool) { assert!(!res.text.contains("$argon2")); }
async fn login_with_correct_password_returns_200_and_cookie(pool: PgPool) { /* ... */ }
async fn login_with_wrong_password_returns_401_invalid_credentials(pool: PgPool) { /* ... */ }
async fn login_unknown_email_returns_same_body_as_wrong_password(pool: PgPool) { assert_eq!(unknown.body, wrong.body); assert_eq!(unknown.status, wrong.status); }
async fn sixth_login_after_five_failures_returns_429_even_with_correct_password(pool: PgPool) {
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.headers["retry-after"], "900");
}
async fn after_20_failures_from_one_ip_any_new_email_gets_429(pool: PgPool) { /* 20 lần sai, mỗi lần một email khác → lần 21 với email mới → 429 */ }
async fn failure_window_expires_after_15_minutes(pool: PgPool) { app.clock.advance(Duration::minutes(15) + Duration::seconds(1)); /* đăng nhập đúng → 200 */ }
async fn successful_login_clears_email_failures(pool: PgPool) { /* 4 lần sai, 1 lần đúng, rồi 4 lần sai nữa → vẫn 401, chưa 429 */ }
async fn attempts_table_stores_only_32_byte_hashes(pool: PgPool) { /* SELECT length(key_hash) → mọi dòng đều = 32 */ }
async fn forwarded_for_is_ignored_when_trust_proxy_is_false(pool: PgPool) { /* 20 lần sai, mỗi lần một email khác và một X-Forwarded-For khác → lần 21 với email mới → 429 theo IP thật */ }
async fn concurrent_wrong_logins_for_one_email_allow_at_most_five_password_checks(pool: PgPool) {
    // 20 request sai mật khẩu gửi cùng lúc (futures::future::join_all trên router đã clone)
    assert!(statuses.iter().filter(|s| **s == StatusCode::UNAUTHORIZED).count() <= 5);
    assert!(statuses.iter().filter(|s| **s == StatusCode::TOO_MANY_REQUESTS).count() >= 15);
}
async fn ipv6_addresses_in_same_64_share_a_limit(pool: PgPool) { /* đổi MockConnectInfo sang 2001:db8::1 và 2001:db8::2 */ }
```

- [ ] **Step 3:** Chạy `cargo test --test integration auth_register_login`. Kết quả mong đợi: FAIL.
- [ ] **Step 4:** Viết code theo các chữ ký trên. Trong `state.rs`, khai báo type alias cho phần ghép dependency, ví dụ `type Sessions = SessionService<PgUserRepository, PgRefreshTokenRepository>;`, rồi dùng `Arc` để chia sẻ handler.
- [ ] **Step 5:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 6: Commit:** `feat(auth): add register and login endpoints with failed-attempt limits`

---

## Task 7: Refresh và đăng xuất

**Files:**
- Modify: `auth/application/services/session.rs` (thêm `refresh`, `end`), `auth/presentation/{routes.rs, handlers.rs}`, `openapi.rs`
- Test: `backend/tests/integration/auth_refresh_logout.rs` (phần HTTP)

**Interfaces:**
- Consumes: `SessionService::start`, `RefreshTokenRepository`, `cookies::*`
- Produces:
  - `SessionService::refresh(&self, presented: &str) -> Result<(User, Session), AuthError>`. Luật theo P3, kiểm tra đúng thứ tự:
    1. Định dạng sai hoặc không tìm thấy token thì trả `InvalidRefreshToken`.
    2. Token đã hết hạn (`now >= expires_at`) thì trả `InvalidRefreshToken`.
    3. Token đã bị thu hồi, `replaced_at` có giá trị và cách hiện tại không quá 30 giây: trả `RefreshConflict`, không thu hồi gì thêm.
    4. Token đã bị thu hồi, `replaced_at` có giá trị và cách hiện tại hơn 30 giây: gọi `revoke_all_for_user`, ghi `tracing::warn!(user_id = %id, "refresh token reuse detected")`, trả `InvalidRefreshToken`.
    5. Token đã bị thu hồi mà `replaced_at` là `NULL` (do đăng xuất, đổi mật khẩu, hoặc bị thu hồi dây chuyền trước đó): trả `InvalidRefreshToken`, **không** thu hồi dây chuyền.
    6. Token hợp lệ: gọi `rotate`, token mới hết hạn sau `now + 30 ngày`. Nếu `rotate` trả `AlreadyRevoked` (hai request chạy đua nhau) thì trả `RefreshConflict`.
    7. Nạp người dùng bằng `find_by_id` để token mới mang vai trò hiện tại trong DB. Không còn tài khoản thì trả `InvalidRefreshToken`.
  - `SessionService::end(&self, presented: &str) -> Result<(), AuthError>`: tìm thấy token thì `revoke`, không tìm thấy thì bỏ qua.
  - `POST /api/v1/auth/refresh`: đọc cookie `dn_refresh` qua `axum_extra::extract::CookieJar`. Thiếu cookie thì trả 401 `invalid_refresh_token`. Thành công thì trả 200 `AuthResponse` kèm cookie mới. **Mọi phản hồi 401 đều kèm `clear_refresh_cookie`**, để trình duyệt không gửi lại cookie hỏng mãi. Phản hồi 409 thì giữ nguyên cookie.
  - `POST /api/v1/auth/logout`: cookie không bắt buộc. Có cookie thì gọi `end`; `end` không tìm thấy token thì bỏ qua, không báo lỗi. Luôn trả 204 kèm cookie xóa (`Max-Age=0`).

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi:**

```rust
async fn refresh_with_valid_cookie_returns_new_token_and_rotates_cookie(pool: PgPool) { assert_ne!(new_cookie, old_cookie); assert!(res.body["access_token"].is_string()); }
async fn refresh_with_old_cookie_within_30s_returns_409_refresh_conflict_and_keeps_session(pool: PgPool) {
    assert_eq!(old.body["error"]["code"], "refresh_conflict");
    assert_eq!(app.post("/api/v1/auth/refresh").cookie(&new_cookie).send().await.status, StatusCode::OK);
}
async fn refresh_with_old_cookie_after_30s_revokes_all_sessions(pool: PgPool) { /* advance 31s → cookie cũ 401; cookie mới cũng 401 */ }
async fn refresh_401_clears_the_cookie(pool: PgPool) { assert!(res.set_cookie("dn_refresh").unwrap().contains("Max-Age=0")); }
async fn token_revoked_by_logout_does_not_revoke_other_sessions(pool: PgPool) { /* đăng nhập 2 lần (A, B) → logout A → advance 31s → refresh bằng A → 401; refresh bằng B → 200 */ }
async fn refresh_after_30_days_returns_401(pool: PgPool) { app.clock.advance(Duration::days(30)); /* ... */ }
async fn refresh_without_cookie_returns_401_invalid_refresh_token(pool: PgPool) { /* ... */ }
async fn refresh_returns_current_role_from_database(pool: PgPool) { /* set_role(Admin) xong mới refresh → user.role == "admin" */ }
async fn logout_revokes_token_and_clears_cookie(pool: PgPool) { assert!(c.contains("Max-Age=0")); /* refresh bằng cookie cũ → 401 */ }
async fn logout_without_cookie_returns_204(pool: PgPool) { /* ... */ }
```

- [ ] **Step 2:** Chạy `cargo test`. Kết quả mong đợi: FAIL.
- [ ] **Step 3:** Viết `refresh`, `end` và hai handler.
- [ ] **Step 4:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 5: Commit:** `feat(auth): add refresh rotation with reuse detection and logout`

---

## Task 8: Người dùng hiện tại, giới tính, kiểm tra quyền admin, lệnh `make_admin`

**Files:**
- Create: `auth/presentation/extractors.rs`, `users/application/{get_current_user/*, update_gender/*}`, `users/presentation/{routes.rs, handlers.rs}`, `modules/admin/{mod.rs, presentation/{mod.rs, extractors.rs}}`, `backend/src/bin/make_admin.rs`
- Modify: `users/presentation/{mod.rs, dto.rs}` (đã tạo ở Task 6), `state.rs`, `routes.rs`, `openapi.rs`
- Test: `backend/tests/integration/me_and_admin.rs`

**Interfaces:**
- Consumes: `JwtService::verify`, `Clock`, `UserRepository`, `Actor`
- Produces:
  - `auth::presentation::extractors::CurrentUser(pub Actor)`, implement `FromRequestParts<AppState>`. Đọc header `Authorization: Bearer <jwt>`. Thiếu header, sai định dạng, token sai hoặc hết hạn đều trả `ApiError::Unauthorized("unauthorized")`.
  - `admin::presentation::extractors::AdminActor(pub Actor)`. Lấy `CurrentUser`, rồi **đọc lại vai trò từ DB** bằng `find_by_id` (P7). Không phải `Admin` thì trả `ApiError::Forbidden`. Người dùng không còn tồn tại thì trả 401. Từ kế hoạch 2, các use case quản trị còn tự gọi `actor.require_admin()` ở tầng application.
  - `GetCurrentUserQuery { actor_id: Uuid }` và `GetCurrentUserHandler::execute -> Result<User, UsersError>`.
  - `UpdateGenderCommand { actor_id: Uuid, gender: Option<String> }` và `UpdateGenderHandler::execute -> Result<User, UsersError>`. Giá trị lạ trả `Validation{"gender": ["invalid_choice"]}`.
  - `UsersError::{Validation(FieldErrors), NotFound, Internal(..)}`, có `From<UsersError> for ApiError`. `NotFound` ánh xạ sang 401 `unauthorized`, vì token còn hợp lệ mà tài khoản không còn.
  - `GET /api/v1/me` trả `UserResponse`.
  - `PATCH /api/v1/me` nhận body `{"gender": <chuỗi> | null}` và trả `UserResponse` (Task 6).
    - DTO là `UpdateMeRequest { #[serde(default, deserialize_with = "double_option")] gender: Option<Option<String>> }`, có `deny_unknown_fields`.
    - `double_option` là một hàm nhỏ trong `shared::extract`, khoảng 5 dòng: đọc giá trị rồi bọc trong `Some`. Nhờ vậy phân biệt được ba trường hợp:
      - thiếu trường `gender`: `None`, trả 422 `{"gender": ["required"]}`;
      - `"gender": null`: `Some(None)`, nghĩa là bỏ chọn;
      - `"gender": "female"`: `Some(Some("female"))`.
  - `TestRequest::cookie(&str)` nhận chuỗi dạng `"dn_refresh=<giá trị>"`. `TestResponse::cookie_value("dn_refresh")` trả phần giá trị của cookie đó trong header `Set-Cookie`.
  - `CurrentUser` dùng `state.jwt` và `state.clock`. `AdminActor` dùng `state.users_repo`.
  - Binary `make_admin`, dùng `anyhow` vì là binary. Đọc `Config`, kết nối DB, gọi `PgUserRepository::set_role(&email, Role::Admin)`. Có tài khoản thì in "Đã cấp quyền admin cho <email>" ra stdout và thoát với mã 0. Không có thì in lỗi ra stderr và thoát với mã 1.
  - Đường dẫn chỉ dùng để test: `tests/integration/me_and_admin.rs` tự dựng router có `GET /probe-admin` dùng `AdminActor`. **Production không có route nào chỉ để thử quyền.**

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi:**

```rust
async fn me_without_token_returns_401(pool: PgPool) { /* ... */ }
async fn me_with_garbage_bearer_returns_401(pool: PgPool) { /* ... */ }
async fn me_with_expired_token_returns_401(pool: PgPool) { app.clock.advance(Duration::minutes(15)); /* ... */ }
async fn me_returns_current_user_without_hash(pool: PgPool) { assert_eq!(res.body["email"], "an@example.com"); assert!(!res.text.contains("password")); }
async fn patch_me_sets_female_then_clears_with_null(pool: PgPool) { assert_eq!(a.body["gender"], "female"); assert!(b.body["gender"].is_null()); }
async fn patch_me_rejects_unknown_gender_with_422(pool: PgPool) { assert_eq!(res.body["error"]["fields"]["gender"][0], "invalid_choice"); }
async fn patch_me_with_empty_body_returns_422_required_and_keeps_gender(pool: PgPool) { assert_eq!(res.body["error"]["fields"]["gender"][0], "required"); }
async fn me_responses_have_cache_control_no_store(pool: PgPool) { assert_eq!(res.headers["cache-control"], "no-store"); }
async fn patch_me_with_role_field_is_rejected_and_role_unchanged(pool: PgPool) { /* {"gender":null,"role":"admin"} → 400; GET /me role vẫn "user" */ }
async fn admin_guard_allows_admin(pool: PgPool) { /* ... */ }
async fn admin_guard_rejects_user_with_403(pool: PgPool) { /* ... */ }
async fn admin_guard_rejects_anonymous_with_401(pool: PgPool) { /* ... */ }
async fn admin_guard_rejects_demoted_admin_even_with_old_token(pool: PgPool) { /* lấy token khi đang là admin → set_role(User) → 403 */ }
```

- [ ] **Step 2:** Chạy `cargo test`. Kết quả mong đợi: FAIL.
- [ ] **Step 3:** Viết code theo các chữ ký trên.
- [ ] **Step 4:** Chạy `cargo test`. Kết quả mong đợi: PASS. Thử tay: `cargo run --bin make_admin -- khongco@example.com` phải thoát với mã 1.
- [ ] **Step 5: Commit:** `feat(users): add current user and gender endpoints, admin guard and make_admin cli`

---

## Task 9: Quên mật khẩu và đặt lại mật khẩu

**Files:**
- Create: `backend/migrations/20261125000004_create_password_reset_tokens.sql`
- Create: `auth/application/ports/email_sender.rs`, `backend/src/infrastructure/email.rs`, `auth/domain/repositories/password_reset_repository.rs`, `auth/infrastructure/repositories/pg_password_reset_repository.rs`, `auth/application/{request_password_reset/*, confirm_password_reset/*}`
- Modify: `AppState::new` thêm tham số `mailer: Arc<dyn EmailSender>`, sửa `main.rs` theo. `AuthHandlers` thêm `request_reset` và `confirm_reset`. Config thêm `smtp_host`, `smtp_port`, `smtp_from` (mặc định `localhost`, `1025`, `dateNaow <no-reply@datenaow.local>`) và `reset_token_ttl_mins: i64` (mặc định 30). Sửa `TestApp`: thêm `pub mailer: Arc<FakeMailer>`. `FakeMailer` có `sent() -> Vec<OutgoingEmail>` và `async fn wait_for(&self, n: usize) -> Vec<OutgoingEmail>`, chờ tối đa 2 giây bằng `tokio::sync::Notify` vì email được gửi ở tác vụ nền.
- Test: `backend/tests/integration/password_reset.rs`

**Interfaces:**
- Produces:
  - `OutgoingEmail { to: Email, subject: String, text_body: String }`
  - Port gửi email:

    ```rust
    pub trait EmailSender: Send + Sync {
        fn send(&self, email: OutgoingEmail) -> Pin<Box<dyn Future<Output = Result<(), SendEmailError>> + Send + '_>>;
    }
    ```

    Kiểu lỗi đặt tên `SendEmailError` để không trùng với `EmailError` (lỗi định dạng email) ở Task 3.

  - `infrastructure::email::SmtpEmailSender::new(host: &str, port: u16, from: &str) -> anyhow::Result<Self>`. Dùng `AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host).port(port)`, chỉ để gửi tới Mailpit (P6).
  - `ResetTokenRecord { id: Uuid, user_id: Uuid, expires_at: DateTime<Utc>, used_at: Option<DateTime<Utc>> }`
  - Trait `PasswordResetRepository: Send + Sync + 'static`, gồm:
    - `replace_for_user(user_id, token_hash: [u8; 32], expires_at, now)`: một transaction làm hai việc. Đánh dấu `used_at = now` cho mọi token chưa dùng của người đó, rồi `INSERT` token mới.
    - `find_by_hash(token_hash) -> Option<ResetTokenRecord>`
    - `consume_and_set_password(token_id, user_id, new_hash: &str, now) -> bool`. Hàm này chạy một transaction gồm ba bước:
      1. `UPDATE password_reset_tokens SET used_at = $now WHERE id = $1 AND used_at IS NULL AND expires_at > $now`. Không sửa được dòng nào thì rollback và trả `false`.
      2. `PgUserRepository::set_password_hash_in_tx(&mut tx, user_id, new_hash)` (Task 3, P11).
      3. `UPDATE refresh_tokens SET revoked_at = $now WHERE user_id = $2 AND revoked_at IS NULL`. Bước này không đặt `replaced_at`, nên cookie cũ chỉ nhận 401 chứ không gây thu hồi dây chuyền (Task 7, luật 5).
  - `RequestPasswordResetCommand { email: String, client_ip: IpAddr }` và `RequestPasswordResetHandler::execute -> Result<(), AuthError>`. Thứ tự xử lý:
    1. Email sai định dạng thì trả `Validation{"email": [...]}`.
    2. Gọi `acquire` cho `RESET_PER_EMAIL` và `RESET_PER_IP`. Vượt giới hạn thì trả `RateLimited`. *(#13: tầng 1 `RESET_PER_PAIR` + `RESET_PER_IP`, tầng 2 `RESET_PER_EMAIL`.)*
    3. Không có tài khoản thì trả `Ok(())`.
    4. Có tài khoản thì sinh `OpaqueToken`, rồi gọi `replace_for_user` với hạn 30 phút.
    5. Gửi email bằng `tokio::spawn`, không chờ kết quả, để thời gian phản hồi gần như không khác giữa email có và không có tài khoản (P6). Gửi lỗi thì ghi `tracing::error!(error = %e, "password reset email failed")`, không kèm token và không kèm địa chỉ email. Handler luôn trả `Ok(())`.
  - Nội dung email:
    - Tiêu đề: `"Đặt lại mật khẩu dateNaow"`
    - Thân: `"Bạn (hoặc ai đó) vừa yêu cầu đặt lại mật khẩu.\nMở link này trong 30 phút để đặt mật khẩu mới:\n{APP_BASE_URL}/reset-password#token={token}\nNếu không phải bạn, cứ bỏ qua email này."`
  - `ConfirmPasswordResetCommand { token: String, new_password: String }` và `ConfirmPasswordResetHandler::execute -> Result<(), AuthError>`. Thứ tự xử lý:
    1. Mật khẩu sai thì trả `Validation{"new_password": [...]}`. Kiểm tra mật khẩu **trước** khi đụng tới token, để token vẫn còn dùng được khi người dùng gõ lại.
    2. Token sai định dạng, không tìm thấy, đã dùng hoặc hết hạn (`now >= expires_at`) đều trả `InvalidOrExpiredResetToken`.
    3. Băm mật khẩu mới, rồi gọi `consume_and_set_password`. Hàm trả `false` thì trả `InvalidOrExpiredResetToken`.
    4. Thành công thì `clear` khóa `login:email:<email>`, để người dùng đăng nhập được ngay bằng mật khẩu mới dù trước đó đã nhập sai nhiều lần. *(#13: lệnh nhận thêm `client_ip` và `clear` cả khóa `login:pair:<email>|<IP>` của IP vừa đặt lại.)*
  - Endpoint:
    - `POST /api/v1/auth/password-reset/request` nhận `{ "email" }`, trả 204.
    - `POST /api/v1/auth/password-reset/confirm` nhận `{ "token", "new_password" }`, trả 204.
    - Cả hai được thêm vào `ApiDoc`.

- [ ] **Step 1: Viết migration.** Bảng `password_reset_tokens (id UUID PK DEFAULT uuidv7(), user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, token_hash BYTEA NOT NULL CHECK (length(token_hash) = 32), expires_at TIMESTAMPTZ NOT NULL, used_at TIMESTAMPTZ NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now())`. Thêm `uq_password_reset_tokens_token_hash` và `idx_password_reset_tokens_user_id`.
- [ ] **Step 2: Viết test chạy trước, cho thấy lỗi:**

```rust
async fn request_for_known_email_sends_one_email_with_fragment_link(pool: PgPool) {
    let sent = app.mailer.wait_for(1).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to.as_str(), "an@example.com");
    assert!(sent[0].text_body.contains("http://localhost:5173/reset-password#token="));
}
async fn request_for_unknown_email_returns_204_and_sends_nothing(pool: PgPool) { assert_eq!(res.status, 204); assert!(app.mailer.sent().is_empty()); }
async fn fourth_request_in_an_hour_returns_429(pool: PgPool) { /* ... */ }
async fn confirm_sets_new_password_and_revokes_sessions(pool: PgPool) { /* mật khẩu cũ → 401; mật khẩu mới → 200; cookie refresh cũ → 401 */ }
async fn token_is_single_use(pool: PgPool) { assert_eq!(second.body["error"]["code"], "invalid_or_expired_token"); }
async fn token_expires_after_30_minutes(pool: PgPool) { app.clock.advance(Duration::minutes(30)); /* → 400 */ }
async fn new_request_invalidates_previous_token(pool: PgPool) { /* token thứ nhất → 400, token thứ hai → 204 */ }
async fn short_new_password_returns_422_and_token_stays_usable(pool: PgPool) { /* ... */ }
async fn old_cookie_after_reset_returns_401_without_revoking_new_session(pool: PgPool) { /* reset → đăng nhập lại (cookie mới) → advance 31s → cookie cũ 401 → cookie mới vẫn 200 */ }
async fn reset_clears_login_failure_counter(pool: PgPool) { /* 5 lần sai → reset → đăng nhập bằng mật khẩu mới → 200 */ }
```

- [ ] **Step 3:** Chạy `cargo test`. Kết quả mong đợi: FAIL.
- [ ] **Step 4:** Viết code theo các chữ ký trên. Trong test, lấy token bằng cách đọc thân email qua `FakeMailer::wait_for` và cắt phần sau `#token=`. Test "không gửi gì" thì chờ thêm 100ms rồi mới kiểm tra `sent()` rỗng.
- [ ] **Step 5:** Chạy `cargo test`. Kết quả mong đợi: PASS.
- [ ] **Step 6: Commit:** `feat(auth): add password reset via single-use emailed link`

---

## Task 10: Docker cho backend và kiểm tra OpenAPI đầy đủ

**Files:**
- Create: `backend/Dockerfile`, `backend/.sqlx/` (sinh bằng `cargo sqlx prepare`)
- Modify: `docker-compose.yml` (thêm service `backend`)
- Test: `backend/tests/integration/openapi.rs` (thêm test)

**Interfaces:**
- Produces: `docker compose up --build` chạy được cả ba service. Backend lắng nghe ở cổng 8080.

- [ ] **Step 1: Viết test kiểm tra đủ đường dẫn** (đây là test bảo vệ, có thể PASS ngay từ đầu):

```rust
async fn openapi_lists_all_plan_1_paths(pool: PgPool) {
    for p in ["/api/v1/health", "/api/v1/auth/register", "/api/v1/auth/login", "/api/v1/auth/refresh",
              "/api/v1/auth/logout", "/api/v1/auth/password-reset/request",
              "/api/v1/auth/password-reset/confirm", "/api/v1/me"] {
        assert!(res.body["paths"][p].is_object(), "missing {p}");
    }
}
```

- [ ] **Step 2:** Chạy `cargo test`. Kết quả mong đợi: PASS nếu các task trước đã thêm đủ đường dẫn. Nếu FAIL thì thêm đường dẫn còn thiếu.
- [ ] **Step 3:** Chạy `cargo sqlx prepare` để sinh `.sqlx/`, rồi thử `SQLX_OFFLINE=true cargo build`. Kết quả mong đợi: build xong mà không cần kết nối DB.
- [ ] **Step 4: Viết `Dockerfile` nhiều tầng.**
  - Tầng build dùng image `rust:<phiên bản đã ghim>-slim` với `SQLX_OFFLINE=true` và `cargo build --release --bin datenaow`.
  - Tầng chạy dùng `debian:<bản stable>-slim`, cài `ca-certificates`, chạy bằng người dùng không phải root, `EXPOSE 8080`.
  - Service `backend` trong compose có `depends_on` với `postgres` ở điều kiện `service_healthy`. Biến môi trường lấy từ `env_file: backend/.env`. Khối `environment:` ghi đè `DATABASE_URL` (host `postgres`) và `SMTP_HOST=mailpit`, vì `.env` dùng `localhost` cho lúc chạy `cargo run` trên máy.
- [ ] **Step 5:** Chạy `docker compose up --build -d`, rồi `curl -s http://localhost:8080/api/v1/health`. Kết quả mong đợi: `{"status":"ok","database":"ok"}`.
- [ ] **Step 6: Commit:** `build(backend): add dockerfile, sqlx offline data and compose service`

---

## Task 11: Khung frontend

**Files:**
- Create: toàn bộ `frontend/` theo bản đồ file (ở bước này chỉ có `main.tsx`, `test/setup.ts`, `test/renderApp.tsx`, `app/router.tsx`, `app/providers.tsx`, `app/NotFoundPage.tsx`, `app/layout/*`, `shared/styles/global.css`)
- Test: `frontend/src/app/router.test.tsx`

**Interfaces:**
- Produces:
  - Các script npm: `dev`, `build` (`tsc -b && vite build`), `lint` (`eslint .`), `format` / `format:check` (`prettier --write .` / `prettier --check .`), `typecheck` (`tsc -b --noEmit` hoặc lệnh tương đương mà phiên bản TS đã chọn hỗ trợ), `test` (`vitest run`).
  - `vite.config.ts`: `server.proxy['/api'] = { target: 'http://localhost:8080', ws: true }`. Phần test dùng `environment: 'jsdom'`, `setupFiles: ['src/test/setup.ts']`.
  - Router dùng **declarative mode** của React Router v8, đúng theo DECISIONS #6: dùng `<BrowserRouter>`, `<Routes>`, `<Route>`, tất cả import từ `react-router`. Không dùng `createBrowserRouter`/`RouterProvider` vì đó là *data mode*, kế hoạch này không cần loader hay action.
  - `app/router.tsx` export component `AppRoutes()`, trả về `<Routes>` chứa toàn bộ route của app, và **không** tự bọc router.
    - `main.tsx` render `<BrowserRouter><AppProviders><AppRoutes /></AppProviders></BrowserRouter>`.
    - Test render `<MemoryRouter initialEntries={[...]}><AppProviders><AppRoutes /></AppProviders></MemoryRouter>`.
    - Ở bước này chỉ có route bắt mọi đường dẫn (`*`) trỏ tới `NotFoundPage`, nằm trong `AppLayout`, dùng `<Outlet />`.
  - `src/test/renderApp.tsx` export `renderApp(path: string)` cho các test sau dùng chung.
  - `app/providers.tsx` export `AppProviders({ children })`, gồm `QueryClientProvider` với `retry: 1` cho query và `retry: 0` cho mutation. Task 13 bọc thêm `AuthProvider` vào đây.
  - `AppLayout` ưu tiên màn hình điện thoại: cột rộng tối đa 480px, căn giữa, padding theo `env(safe-area-inset-*)`. `index.html` có `<html lang="vi">` và `<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">`.

- [ ] **Step 1:** Kiểm tra phiên bản bằng `npm view <pkg> version` cho từng package trong bảng phiên bản. Kiểm tra dải `peerDependencies` của `typescript-eslint` để chọn phiên bản TypeScript.
- [ ] **Step 2:** `npm create vite@latest frontend -- --template react-ts`. Cài các thư viện đã duyệt bằng `npm install --save-exact` (`react-router`, `@tanstack/react-query`, cùng các devDependency ở DECISIONS #6 và P13). Thêm `"engines": { "node": ">=24" }` và file `.nvmrc` ghi `24`. Bỏ phần demo của template.
- [ ] **Step 3:** Cấu hình `eslint.config.js` (flat config): `@eslint/js` recommended, `typescript-eslint` recommendedTypeChecked, `eslint-plugin-react-hooks` recommended, `eslint-plugin-react-refresh`, và `eslint-config-prettier` đặt cuối cùng. Tạo `.prettierrc.json` với `singleQuote: true`.
- [ ] **Step 4: Viết test chạy trước, cho thấy lỗi:**

```tsx
test('renders not-found page for unknown route', () => {
  renderApp('/khong-co'); // MemoryRouter + AppProviders + AppRoutes
  expect(screen.getByRole('heading', { name: 'Không tìm thấy trang' })).toBeInTheDocument();
});
```

- [ ] **Step 5:** Chạy `npm test`. Kết quả mong đợi: FAIL. Viết `NotFoundPage` (tiêu đề "Không tìm thấy trang", link "Về trang chủ"), rồi chạy `npm test`. Kết quả mong đợi: PASS.
- [ ] **Step 6:** Chạy `npm run lint && npm run typecheck && npm run build`. Kết quả mong đợi: không lỗi. Ghi phiên bản và các lệnh của frontend vào `.claude/CLAUDE.md` §2.4 và §15.
- [ ] **Step 7: Commit:** `feat(frontend): scaffold vite react app with router, query client and tooling`

---

## Task 12: Trình gọi API và kho access token

**Files:**
- Create: `frontend/src/shared/api/{tokenStore.ts, errors.ts, types.ts, client.ts}`
- Test: `frontend/src/shared/api/client.test.ts`

**Interfaces:**
- Produces:
  - `tokenStore.ts`:
    - `getAccessToken(): string | null`
    - `setAccessToken(token: string | null): void`
    - Token **chỉ nằm trong bộ nhớ**, không bao giờ ghi vào `localStorage` hay `sessionStorage`.
  - `types.ts`:
    - `type Gender = 'male' | 'female' | 'other' | 'prefer_not_to_say'`
    - `interface UserDto { id: string; email: string; role: 'user' | 'admin'; gender: Gender | null }`
    - `interface AuthResponse { access_token: string; token_type: 'Bearer'; expires_in: number; user: UserDto }`
  - `errors.ts`:
    - `class ApiError extends Error { status: number; code: string; fields?: Record<string, string[]>; retryAfterSecs?: number }`
    - `async function toApiError(res: Response): Promise<ApiError>`. Đọc header `Retry-After` vào `retryAfterSecs`. Body không phải JSON thì `code` là `'unknown_error'`. Lỗi mạng (fetch ném lỗi) được bọc thành `ApiError` với `status: 0`, `code: 'network_error'`.
  - `client.ts`:
    - `apiFetch<T>(path: string, options?: { method?: string; json?: unknown; auth?: boolean }): Promise<T>`
      - Tự thêm tiền tố `/api/v1` và luôn gửi `credentials: 'same-origin'`.
      - Mặc định `auth: true`, nghĩa là có token thì gắn `Authorization: Bearer`.
      - Trả 204 thì resolve `undefined`.
      - Gặp 401 khi `auth` đang bật: gọi `refreshAccessToken()` một lần rồi gửi lại request đúng một lần. Refresh thất bại thì `setAccessToken(null)`, báo các listener của `onSessionExpired`, và ném lỗi `ApiError` 401.
    - `refreshAccessToken(): Promise<AuthResponse | null>`
      - Gọi `POST /api/v1/auth/refresh`.
      - **Chỉ một request refresh chạy tại một thời điểm**: mọi lời gọi đến trong lúc chờ dùng chung cùng một Promise.
      - Gặp 409 `refresh_conflict` thì đợi 500ms rồi thử lại đúng một lần, vì lúc đó tab khác vừa đổi cookie xong.
      - Thành công thì `setAccessToken`, thất bại thì trả `null`.
    - `onSessionExpired(listener: () => void): () => void`, trả về hàm hủy đăng ký.

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi** (giả `fetch` bằng `vi.stubGlobal('fetch', vi.fn())`):

```ts
test('adds bearer header when a token is stored')
test('on 401 refreshes once and retries the original request')        // fetch được gọi lần lượt: /me 401 → /auth/refresh 200 → /me 200
test('concurrent 401s trigger a single refresh request')              // 3 lời gọi song song → đúng 1 lần gọi /auth/refresh
test('when refresh fails it clears the token, notifies listeners and throws 401')
test('refresh_conflict is retried once after 500ms')                  // vi.useFakeTimers()
test('parses error body into ApiError with code and fields')
test('reads Retry-After into retryAfterSecs')
test('does not try to refresh when auth is false')                    // ví dụ khi gọi đăng nhập
test('resolves undefined on 204')
```

- [ ] **Step 2:** Chạy `npm test`. Kết quả mong đợi: FAIL. Viết code, rồi chạy `npm test`. Kết quả mong đợi: PASS. Chạy `npm run lint && npm run typecheck`.
- [ ] **Step 3: Commit:** `feat(frontend): add api client with in-memory token and single-flight refresh`

---

## Task 13: Phiên đăng nhập, trang đăng ký và đăng nhập, chặn trang khi chưa đăng nhập

**Files:**
- Create: `frontend/src/features/auth/{api.ts, AuthProvider.tsx, messages.ts, pages/SignInPage.tsx, pages/SignUpPage.tsx}`, `frontend/src/app/guards/RequireAuth.tsx`, `frontend/src/features/home/HomePage.tsx`, `frontend/src/shared/ui/{Button.tsx, TextField.tsx, FormMessage.tsx}` (kèm `*.module.css`)
- Modify: `app/router.tsx`, `app/providers.tsx`
- Test: `SignInPage.test.tsx`, `SignUpPage.test.tsx`, `RequireAuth.test.tsx`, `AuthProvider.test.tsx`

**Interfaces:**
- Consumes: `apiFetch`, `refreshAccessToken`, `onSessionExpired`, `setAccessToken`
- Produces:
  - `features/auth/api.ts`:
    - `register(email: string, password: string): Promise<AuthResponse>`
    - `login(email: string, password: string): Promise<AuthResponse>` (gọi với `auth: false`)
    - `logout(): Promise<void>`
    - `requestPasswordReset(email: string): Promise<void>`
    - `confirmPasswordReset(token: string, newPassword: string): Promise<void>`
  - `AuthProvider` và hook `useAuth()`. Hook trả `{ status: 'loading' | 'authenticated' | 'anonymous'; user: UserDto | null; signIn(email, password): Promise<void>; signUp(email, password): Promise<void>; signOut(): Promise<void>; setUser(user: UserDto): void }`.
    - Trạng thái quản lý bằng `useReducer`.
    - Khi mount thì gọi `refreshAccessToken()`.
    - Đăng ký nghe `onSessionExpired`, khi phiên hết hạn thì chuyển sang `anonymous`.
    - `signOut` gọi `logout()`, xóa token và xóa cache của query client.
  - `RequireAuth`: dùng như một route bọc các route con.
    - Đang `loading` thì hiện "Đang tải…" với `role="status"`.
    - `anonymous` thì `<Navigate to="/sign-in" replace state={{ from: location }} />`.
    - Đã đăng nhập thì render `<Outlet />`.
  - Các route: `/sign-in`, `/sign-up`, và `/` (là `HomePage`, đặt trong `RequireAuth`). Đăng nhập xong thì quay lại trang `state.from`, mặc định là `/`.
  - `messages.ts` có hai hàm, chữ trên giao diện theo P10.
    - `errorMessage(err: ApiError): string` ánh xạ mã lỗi chung:
      - `invalid_credentials`: "Email hoặc mật khẩu chưa đúng."
      - `email_taken`: "Email này đã có tài khoản. Bạn đăng nhập nhé."
      - `rate_limited`: "Bạn thử nhiều lần quá. Đợi khoảng {n} phút rồi thử lại nhé." Trong đó `n = Math.ceil(retryAfterSecs / 60)`, thiếu header thì lấy 15.
      - `network_error`: "Không kết nối được. Bạn kiểm tra mạng rồi thử lại nhé."
      - Mặc định: "Đã có lỗi, bạn thử lại sau nhé."
    - `fieldMessage(field: string, code: string): string` ánh xạ theo **cả tên trường và mã**:
      - `email` + `required`: "Bạn nhập email nhé."
      - `email` + `invalid_email`: "Email chưa đúng định dạng."
      - `email` + `too_long`: "Email dài quá."
      - `password` hoặc `new_password` + `too_short`: "Mật khẩu cần ít nhất 8 ký tự."
      - `password` hoặc `new_password` + `too_long`: "Mật khẩu dài tối đa 128 ký tự."
      - Mặc định: "Giá trị chưa hợp lệ."
  - Form có `<label>` cho từng ô, `autocomplete="email"` và `autocomplete="current-password"` hoặc `"new-password"`. Nút gửi bị khóa trong lúc chờ. Kiểm tra độ dài mật khẩu ngay trên trình duyệt trước khi gọi API.
  - `HomePage` hiện "Xin chào, {email}", link "Cài đặt" và nút "Đăng xuất". Các kế hoạch sau sẽ thay nội dung trang này.

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi** (giả `features/auth/api` bằng `vi.mock`):

```tsx
test('SignInPage shows password length error without calling the api')
test('SignInPage signs in and navigates to the page the user came from')
test('SignInPage shows invalid_credentials message')
test('SignInPage shows rate_limited message with minutes from Retry-After')   // retryAfterSecs 900 → "15 phút"
test('SignUpPage shows field messages from server 422 by field and code')
test('SignUpPage shows email_taken message')
test('SignUpPage signs up and lands on home greeting the user')
test('RequireAuth shows loading status while the session restores')
test('RequireAuth redirects anonymous users to /sign-in')
test('AuthProvider restores the session on mount via refresh')
test('session expiry while on home redirects once to /sign-in')        // phát sự kiện onSessionExpired → đúng một lần chuyển trang, không lặp
```

- [ ] **Step 2:** Chạy `npm test`. Kết quả mong đợi: FAIL. Viết code, rồi chạy `npm test`. Kết quả mong đợi: PASS. Chạy `npm run lint && npm run typecheck`.
- [ ] **Step 3: Commit:** `feat(frontend): add auth session, sign-in and sign-up pages and route guard`

---

## Task 14: Quên mật khẩu, đặt lại mật khẩu, cài đặt giới tính, trang `/admin`

**Files:**
- Create: `features/auth/pages/{ForgotPasswordPage.tsx, ResetPasswordPage.tsx}`, `features/settings/{api.ts, SettingsPage.tsx}`, `app/guards/RequireAdmin.tsx`, `features/admin/AdminHomePage.tsx`
- Modify: `app/router.tsx`
- Test: `ForgotPasswordPage.test.tsx`, `ResetPasswordPage.test.tsx`, `SettingsPage.test.tsx`, `RequireAdmin.test.tsx`

**Interfaces:**
- Produces:
  - `features/settings/api.ts`: `updateGender(gender: Gender | null): Promise<UserDto>` gọi `PATCH /me`.
  - `ForgotPasswordPage` (`/forgot-password`): gửi xong thì **luôn** hiện "Nếu email này có tài khoản, bạn sẽ nhận được link đặt lại mật khẩu trong vài phút.". Chỉ riêng 429 thì hiện câu `rate_limited` của `errorMessage`, với số phút lấy từ `Retry-After` (thường là 60 phút cho trang này).
  - `ResetPasswordPage` (`/reset-password`):
    - Đọc token từ `location.hash` (`#token=`), rồi `window.history.replaceState` để xóa phần hash khỏi thanh địa chỉ.
    - Có hai ô "Mật khẩu mới" và "Nhập lại mật khẩu". Hai ô phải khớp nhau, kiểm tra ngay trên trình duyệt.
    - Thành công thì hiện "Đổi mật khẩu xong. Mời bạn đăng nhập lại." kèm link `/sign-in`.
    - Lỗi `invalid_or_expired_token` thì hiện "Link này đã hết hạn hoặc đã được dùng. Bạn xin link mới nhé." kèm link `/forgot-password`.
    - Không có token thì hiện ngay câu lỗi đó.
  - `SettingsPage` (`/settings`, nằm trong `RequireAuth`):
    - Nhóm radio "Giới tính" gồm Nam → `male`, Nữ → `female`, Khác → `other`, Không muốn nói → `prefer_not_to_say`, cùng nút "Bỏ chọn" gửi `null`.
    - Dòng ghi chú: "Mèo dùng mục này để biết nên nhắc bạn vào ngày lễ nào. Người ấy không bao giờ thấy mục này."
    - Dùng `useMutation`. Thành công thì gọi `setUser`.
  - `RequireAdmin` (đặt trong `RequireAuth`): người dùng không phải `admin` thì hiện trang "Bạn không có quyền vào trang này." kèm link về trang chủ. Đây chỉ là trải nghiệm giao diện, còn việc chặn thật nằm ở API.
  - `AdminHomePage` (`/admin`): tiêu đề "Trang quản trị" và dòng "Các mục quản trị sẽ có ở các bản sau.".

- [ ] **Step 1: Viết test chạy trước, cho thấy lỗi:**

```tsx
test('ForgotPasswordPage shows the same confirmation whether or not the email exists')
test('ForgotPasswordPage shows 60 minutes when rate limited with Retry-After 3600')
test('ResetPasswordPage reads the token from the hash, strips it, and submits')
test('ResetPasswordPage shows the expired message on invalid_or_expired_token')
test('ResetPasswordPage rejects mismatched passwords before calling the api')
test('SettingsPage selecting Nữ sends female and shows it as selected')
test('SettingsPage Bỏ chọn sends null')
test('RequireAdmin shows forbidden page to a non-admin user')
test('RequireAdmin renders admin home for an admin user')
```

- [ ] **Step 2:** Chạy `npm test`. Kết quả mong đợi: FAIL. Viết code, rồi chạy `npm test`. Kết quả mong đợi: PASS. Chạy `npm run lint && npm run typecheck && npm run build`.
- [ ] **Step 3: Commit:** `feat(frontend): add password reset, gender settings and admin guard pages`

---

## Task 15: Kiểm tra toàn bộ, tài liệu và review

**Files:**
- Create: `README.md` (cách chạy dự án trên máy, ngắn gọn)
- Modify: `.claude/CLAUDE.md` (§0.2 trạng thái, §2.4 phiên bản, §15 lệnh của frontend), `docs/DECISIONS.md` (bổ sung quyết định phát sinh trong lúc làm, nếu có)

- [ ] **Step 1:** Chạy đầy đủ các lệnh kiểm tra.
  - Trong `backend/`: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo check`, `cargo test`, `cargo build`.
  - Trong `frontend/`: `npm run lint`, `npm run format:check`, `npm run typecheck`, `npm test`, `npm run build`.
  - Kết quả mong đợi: tất cả đều qua. Ghi lại output, lệnh nào lỗi thì báo nguyên văn.
- [ ] **Step 2: Kiểm tra tay** ở khung hình điện thoại 375×812. Chạy `docker compose up -d`, rồi `npm run dev` và mở `http://localhost:5173`:
  1. Đăng ký thì vào trang chủ, thấy "Xin chào, …".
  2. Tải lại trang thì vẫn còn đăng nhập.
  3. Đăng xuất thì về trang `/sign-in`.
  4. Đăng nhập sai 6 lần thì thấy câu báo "thử sai nhiều lần".
  5. Vào "Quên mật khẩu?", mở email trong Mailpit (`http://localhost:8025`), bấm link, đặt mật khẩu mới, rồi đăng nhập bằng mật khẩu mới.
  6. Trong Cài đặt, chọn "Nữ", tải lại trang thì vẫn là "Nữ".
  7. Vào `/admin` bằng tài khoản thường thì thấy trang báo không có quyền. Chạy `cargo run --bin make_admin -- <email>`, đăng xuất rồi đăng nhập lại, vào `/admin` thì thấy "Trang quản trị".
  8. Mở hai tab, đợi 15 phút (hoặc tạm đặt `ACCESS_TOKEN_TTL_SECS=60`), thao tác ở cả hai tab: không tab nào bị đăng xuất.
- [ ] **Step 3:** Cập nhật `.claude/CLAUDE.md` và `README.md`. Kiểm tra `docs/DECISIONS.md` đã có P1-P13 (ghi ở Task 0) và mọi phương án dự phòng phiên bản đã dùng.
- [ ] **Step 4:** Nhờ subagent `code-reviewer` review toàn bộ nhánh, chú trọng phần bảo mật: JWT, cookie, refresh, giới hạn số lần thử, đặt lại mật khẩu, việc không lộ mã băm và token trong log. Sửa những lỗi review tìm ra, kèm test.
- [ ] **Step 5: Commit** (khi được đồng ý): `docs: record plan 1 decisions, versions and local run instructions`
- [ ] **Step 6:** Báo cáo cho chủ dự án theo mẫu ở global CLAUDE.md §14. Hỏi chủ dự án có muốn push hoặc mở PR không, và có bắt đầu viết chi tiết kế hoạch 2 không.
