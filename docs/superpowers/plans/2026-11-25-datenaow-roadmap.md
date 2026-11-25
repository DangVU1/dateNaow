# dateNaow: Lộ trình triển khai

*Ngày: 2026-11-25 · Spec: `docs/superpowers/specs/2026-11-23-datenaow-design.md` (đã duyệt) · Quyết định chia kế hoạch: `docs/DECISIONS.md` #5*

Bản đầu tiên được làm qua **bốn kế hoạch nối tiếp nhau**. Mỗi kế hoạch khi xong là một phần chạy được, có test, và dùng được trên điện thoại. Lúc này chỉ kế hoạch 1 được viết chi tiết. Mỗi kế hoạch sau sẽ được viết chi tiết ngay trước khi làm, dựa trên code thật của các kế hoạch trước, rồi review và trình chủ dự án duyệt giống kế hoạch 1.

Ví dụ dễ hình dung: xây nhà thì đổ móng và làm cửa ra vào trước (kế hoạch 1), xong mới vẽ chi tiết từng phòng. Lúc đó đã biết móng nằm đâu, nên bản vẽ phòng không bị lệch.

## Thứ tự và phụ thuộc

```
Kế hoạch 1 ──► Kế hoạch 2 ──► Kế hoạch 3 ──► Kế hoạch 4
 nền móng        cặp đôi,       địa điểm,      quà, dịp tặng,
 + tài khoản     ghi chú,       buổi hẹn,      chú mèo
                 mong muốn      chat chọn chỗ
```

Thứ tự này bắt buộc. Kế hoạch 3 cần cặp đôi và thẻ sở thích của kế hoạch 2. Kế hoạch 4 cần mong muốn (kế hoạch 2) và buổi hẹn (kế hoạch 3).

---

## Kế hoạch 1: Nền móng và tài khoản

**File:** `2026-11-25-plan-1-foundation-accounts.md` (đã viết chi tiết)

- **Mục tiêu:** dựng khung backend và frontend, rồi làm trọn phần tài khoản: đăng ký, đăng nhập, giữ phiên đăng nhập, đăng xuất, quên mật khẩu, cài đặt giới tính và vai trò `admin`.
- **Module:** `auth`, `users`, cùng phần dùng chung (`config`, `shared`, `infrastructure`). Ở tầng web có bộ kiểm tra quyền `admin`.
- **Spec:** §3, §6.1, §8 (hạ tầng, cùng origin, Docker Compose).
- **Xong khi:**
  - `docker compose up` chạy được PostgreSQL, Mailpit và backend.
  - Trên điện thoại (hoặc chế độ mobile của trình duyệt), người dùng đăng ký, đăng nhập, tải lại trang mà vẫn còn đăng nhập, đăng xuất được.
  - Đặt lại mật khẩu qua email nhận trong Mailpit.
  - Đổi được giới tính.
  - `/admin` chỉ mở được với tài khoản `admin`.
  - Toàn bộ test và lint của backend và frontend đều qua.

## Kế hoạch 2: Cặp đôi, "Về người ấy", mong muốn, bộ phân tích câu

- **Mục tiêu:**
  - Liên kết hai người bằng mã mời hoặc link mời, và hủy liên kết, kèm câu hỏi làm mới hoặc cập nhật ghi chú.
  - Ngày bắt đầu yêu nhau.
  - Kho "Về người ấy", có ngày sinh của người ấy.
  - Kho mong muốn: mỗi mong muốn là một dòng chữ tự do.
  - Bộ phân tích câu bằng từ điển từ khóa.
  - Trang quản trị cho từ điển và danh sách từ khóa chưa nhận ra.
- **Module:** `couples`, `partner_notes`, `wishes`, `text_analysis`, và route quản trị tương ứng trong `admin`.
- **Spec:** §6.2, §6.3, §6.4, §7.2, phần từ điển ở §6.8.
- **Các mục §9 sẽ đề xuất trước khi viết chi tiết:**
  - tên bảng và cột của các module này;
  - bộ từ điển nhỏ để seed test;
  - bộ câu của mèo khi hỏi làm mới hoặc cập nhật ghi chú.
- **Ghi chú phụ thuộc:**
  - Spec §6.2 nói hủy liên kết thì hủy luôn các buổi hẹn chưa diễn ra. Phần này làm ở kế hoạch 3, vì đến lúc đó mới có buổi hẹn.
  - Khung nói của mèo ở kế hoạch 2 là bản đơn giản, dùng ảnh tạm, chỉ để hỏi làm mới hoặc cập nhật ghi chú. Bản hoàn chỉnh làm ở kế hoạch 4.
- **Xong khi:**
  - Hai tài khoản liên kết và hủy liên kết được.
  - Mong muốn và ghi chú được phân tích ra thẻ, giá và mức "rất muốn".
  - Có test chứng minh người kia không đọc được ghi chú và mong muốn qua bất kỳ API nào, kể cả sau khi đã liên kết.
  - Có test chứng minh **giới tính** và **ngày sinh trong "Về người ấy"** không có trong bất kỳ phản hồi API nào mà người kia nhận được (spec §6.1).
  - Có test chứng minh quản trị viên không thấy được câu gốc của người dùng.

## Kế hoạch 3: Địa điểm, buổi hẹn, khung chat chọn địa điểm

- **Mục tiêu:**
  - Kho địa điểm do quản trị viên nhập (thêm, sửa, ẩn, tải ảnh).
  - Gợi ý địa điểm theo §7.1.
  - Buổi hẹn có hai chế độ: cùng chọn và bất ngờ. Ngân sách quà là của riêng từng người.
  - Khung chat chọn địa điểm qua WebSocket: lựa chọn đang chờ, hiệu ứng "Chốt kèo! 🎉", câu nhắn nhanh, tự nhập địa điểm ngoài kho.
  - Hủy liên kết thì hủy các buổi hẹn chưa diễn ra.
  - Test end-to-end bằng Playwright.
- **Module:** `places`, `date_plans`, và route quản trị địa điểm.
- **Spec:** §6.5, §6.6, §6.8 (địa điểm), §7.1, phần WebSocket ở §8.
- **Các mục §9 sẽ đề xuất trước khi viết chi tiết:**
  - tên bảng và cột;
  - định dạng tin nhắn WebSocket;
  - bộ câu nhắn nhanh;
  - thời gian và đường cong easing của hiệu ứng màn hình mờ, thẻ nảy lên và tim bay;
  - cách xử lý ảnh địa điểm (giới hạn kích thước, định dạng).
- **Phụ thuộc cần duyệt:** Playwright (dev dependency của frontend).
- **Xong khi:**
  - Hai người cùng chọn được địa điểm trên hai điện thoại, kể cả khi một người mở app sau.
  - Buổi hẹn bất ngờ hoàn toàn vô hình với người kia, có test.
  - Gợi ý địa điểm đúng các bước lọc và xếp hạng của §7.1, có test unit cho từng bước.

## Kế hoạch 4: Quà, dịp tặng, chú mèo

- **Mục tiêu:**
  - Kho ý tưởng quà do quản trị viên nhập.
  - Dịp tặng quà theo §7.3: ngày lễ, bảng Thất tịch 2026-2035, ngày lễ theo giới tính, sinh nhật, ngày kỷ niệm.
  - Nhắc trước 7 ngày và 3 ngày, cùng cách hiểu "đã chuẩn bị".
  - Mèo hỏi ngân sách quà khi buổi hẹn hoặc dịp chưa có ngân sách. Ngân sách được lưu cho đúng dịp đó (§6.7, §7.3).
  - Gợi ý quà ngẫu nhiên tối đa 3 lần, chia theo 3 đoạn thời gian.
  - Người dùng tự hỏi gợi ý quà.
  - Chấm điểm và chọn quà theo §7.4, có phương án dự phòng.
  - Lịch sử "đã gợi ý".
  - Mèo pixel hoàn chỉnh: sprite, vài khung hình động, khung nói thu gọn được.
- **Module:** `gifts`, và route quản trị kho quà.
- **Spec:** §6.7, §7.3, §7.4, phần kho quà ở §6.8.
- **Ghi chú phụ thuộc:** spec §6.2 nói "làm mới ghi chú" khi hủy liên kết thì xóa luôn lịch sử "đã gợi ý" của cặp cũ. Kế hoạch 2 làm luồng làm mới ghi chú, nhưng đến kế hoạch 4 mới có lịch sử gợi ý. Vì vậy kế hoạch 4 phải mở rộng luồng đó để xóa cả lịch sử, kèm test.
- **Các mục §9 sẽ đề xuất trước khi viết chi tiết:**
  - tên bảng và cột;
  - bộ mẫu câu thoại của mèo;
  - bộ ý tưởng quà nhỏ để seed test;
  - hiệu ứng của mèo.
- **Xong khi:**
  - Mèo xuất hiện đúng lúc theo §7.3, có test với đồng hồ giả để kiểm tra từng mốc ngày.
  - Quà được chọn đúng theo bảng điểm §7.4.
  - Có test chứng minh người tặng không thấy câu gốc, và người nhận không biết mong muốn nào đã được gợi ý.
  - Có test chứng minh ngân sách quà của người này không bao giờ hiện cho người kia.

---

## Ngoài bốn kế hoạch (làm sau, khi chủ dự án muốn triển khai thật)

- Reverse proxy (Caddy hoặc Nginx) phục vụ bản build của frontend và chuyển `/api` cùng WebSocket sang backend (spec §8).
- Nhà cung cấp SMTP thật, chuyển ảnh sang object storage, sao lưu cơ sở dữ liệu.
- Pipeline CI (GitHub Actions hoặc công cụ tương tự).

Các mục này cần một kế hoạch riêng và được duyệt riêng.

## Rủi ro chung

| Rủi ro | Cách giảm |
|---|---|
| Một số thư viện vừa lên phiên bản lớn: sqlx 0.9, jsonwebtoken 12, utoipa 6, TypeScript 7, React Router 8 | Kế hoạch 1 có bước kiểm tra phiên bản và độ tương thích khi dựng khung. Gặp xung đột thì dùng bản ổn định gần nhất và ghi vào `.claude/CLAUDE.md` §2.4. |
| Lộ thông tin riêng tư giữa hai người (spec §2, `.claude/CLAUDE.md` §0.3) | Mỗi kế hoạch có test "được xem / bị chặn" cho chủ dữ liệu, người kia và người lạ. Reviewer kiểm tra riêng phần này. |
| Nội dung (từ điển, kho quà, địa điểm) chưa có | Chủ dự án hoặc quản trị viên soạn. Code chỉ seed một bộ nhỏ để test (spec §9). |
