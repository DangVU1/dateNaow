# dateNaow: Thiết kế sản phẩm

*Ngày: 2026-11-23 · Trạng thái: bản nháp, chờ duyệt*

Ghi chú cách đọc: mục nào **không** có nhãn là quyết định của chủ dự án. Mục có nhãn **[Đề xuất]** là quyết định do người viết tài liệu đưa ra, chủ dự án có thể sửa.

---

## 1. Mục đích

dateNaow là trang web giúp **các cặp đôi đang yêu và các cặp mới quen** làm ba việc:

1. **Chọn địa điểm và hoạt động đi date** hợp với ngân sách, vị trí của cả hai và đặc điểm của người ấy.
2. **Chọn quà** hợp với ngân sách người tặng và mong muốn thật của người nhận, mà **vẫn giữ được bất ngờ**.
3. **Ghi lại đặc điểm** của người ấy, để các gợi ý ngày càng đúng hơn.

Hai câu hỏi trung tâm là "đi đâu?" và "tặng gì?". Lời đáp được đưa ra nhẹ nhàng qua một **chú mèo pixel** ló ra từ mép màn hình.

## 2. Mục tiêu, ngoài phạm vi, điều phải tránh

### Mục tiêu (bản đầu tiên)
- Tài khoản bằng email và mật khẩu. Liên kết hai tài khoản thành một cặp bằng mã mời.
- Hai kho thông tin riêng tư cho mỗi người: **"Về người ấy"** và **"Người kia muốn gì"**.
- Lập buổi hẹn theo hai chế độ:
  - **Cùng chọn:** chọn địa điểm trong khung chat, có hiệu ứng "match".
  - **Bất ngờ:** người lên kế hoạch chọn từ gợi ý của hệ thống.
- Gợi ý địa điểm từ **kho địa điểm tự chọn lọc ở Sài Gòn**.
- Gợi ý quà bằng chấm điểm theo luật. Mèo nói câu "Bạn có biết?" vào các dịp: buổi hẹn, ngày lễ của cặp đôi, sinh nhật.
- Trang quản trị để nhập địa điểm, kho ý tưởng quà và từ điển từ khóa.

### Ngoài phạm vi (bản đầu)
- Gợi ý cửa hàng mua quà. Người tặng tự tìm chỗ mua.
- Chat tự do giữa hai người.
- Thông báo đẩy và email nhắc trước dịp. Chỉ gửi email khi đặt lại mật khẩu.
- Đăng nhập bằng Google hoặc OTP qua số điện thoại. Sẽ thêm Google sau.
- Dùng AI hay LLM để phân tích mong muốn hoặc viết câu thoại.
- Người dùng tự đề xuất địa điểm.
- Các thành phố ngoài Sài Gòn.
- Ứng dụng di động native và thanh toán.

### Điều phải tránh (dù code chạy đúng vẫn coi là thất bại)
- **Lộ bất ngờ:**
  - Người nhận thấy được ghi chú "Về người ấy" mà người kia viết về mình.
  - Người nhận biết món quà nào được gợi ý từ danh sách mong muốn của mình.
  - Người nhận thấy buổi hẹn bất ngờ.
- **Mèo che màn hình:** mèo và khung nói phải nhỏ, chỉ chiếm một góc.
- **Bắt người dùng điền form dài để ghi mong muốn:** mong muốn chỉ là một dòng chữ tự do.

## 3. Ràng buộc

- **Backend:** Rust bản stable, Axum, Tokio, PostgreSQL, SQLx (kèm SQLx Migrate), Serde, validator, JWT, Argon2, thiserror/anyhow, tracing, Utoipa, Docker.
- **Kiến trúc backend:** modular monolith theo Clean Architecture, xem `.claude/CLAUDE.md`. Chỉ dùng CQRS khi có lý do. Áp dụng các quy tắc chống over-engineering.
- **Frontend:** React, Vite và TypeScript. Ứng dụng web ưu tiên màn hình điện thoại.
- **Phạm vi địa lý:** chỉ Sài Gòn (TP.HCM) trước, sau này mở rộng sang nơi khác.
- **Tiền tệ:** VND.

## 4. Các hướng đã cân nhắc

| | Hướng 1: Google Places + luật | **Hướng 2: Kho tự chọn lọc + luật** ✅ | Hướng 3: Google Places + LLM |
|---|---|---|---|
| Địa điểm | Google Places API | Kho do quản trị viên tự nhập | Google Places API |
| Chọn quà | Chấm điểm theo luật | Chấm điểm theo luật | LLM chọn và viết thoại |
| Ưu | Phủ cả nước ngay | Giá đúng, gợi ý đúng chất "date", không tốn phí API | Nhập liệu tự nhiên nhất, thoại sinh động |
| Nhược | Tốn phí theo lượt gọi, thông tin giá ở Việt Nam hay bị thiếu | Phải tự nhập dữ liệu, ban đầu chỉ phủ Sài Gòn | Tốn phí, khó viết test, gửi dữ liệu riêng tư ra bên ngoài |

**Chọn hướng 2**, làm ở Sài Gòn trước. Phân tích mong muốn dạng câu tự do được giải quyết bằng **từ điển từ khóa** (mục 7.2), không dùng AI.

## 5. Khái niệm chính

| Khái niệm | Ý nghĩa |
|---|---|
| **Người dùng** | Một tài khoản. Có vai trò `user` hoặc `admin`. |
| **Cặp đôi** | Liên kết giữa hai người dùng, giữ nguyên cho đến khi một người hủy. Mỗi người tại một thời điểm chỉ thuộc tối đa một cặp. |
| **"Về người ấy"** | Ghi chú của người dùng A về người ấy: sở thích, đặc điểm, sinh nhật. **Chỉ A thấy**, kể cả khi đã liên kết. |
| **"Người kia muốn gì"** (mong muốn) | Các dòng mong muốn mà **chính người dùng tự ghi** về những gì mình muốn được tặng. Sửa và xóa được. Người kia **không bao giờ thấy** danh sách này, chỉ nhận một gợi ý đã chọn sẵn. |
| **Buổi hẹn** | Có ngày giờ, ngân sách đi chơi, ngân sách quà và chế độ (cùng chọn hoặc bất ngờ). |
| **Dịp tặng quà** | Buổi hẹn, ngày lễ của cặp đôi, sinh nhật người ấy hoặc ngày kỷ niệm. |
| **Địa điểm** | Một mục trong kho do quản trị viên nhập. |
| **Ý tưởng quà** | Một mục trong kho quà chọn lọc, dùng khi không có mong muốn nào phù hợp. |

## 6. Tính năng chi tiết

### 6.1 Tài khoản và đăng nhập
- Đăng ký và đăng nhập bằng email và mật khẩu. Mật khẩu được băm bằng Argon2. Phiên đăng nhập dùng JWT.
- Quên mật khẩu: gửi email chứa link đặt lại. Link dùng một lần và có hạn dùng.
- **[Đề xuất]** Access token sống ngắn (khoảng 15 phút). Refresh token sống dài (khoảng 30 ngày), được lưu trong cookie httpOnly và có thể thu hồi. Như vậy người dùng không phải đăng nhập lại thường xuyên.
- **[Đề xuất]** Gửi email bằng SMTP qua crate `lettre`. Môi trường phát triển dùng Mailpit trong docker-compose. Nhà cung cấp SMTP thật do người triển khai chọn.
- **[Đề xuất]** Giới hạn số lần đăng nhập sai theo IP và theo email.

### 6.2 Liên kết hai người
- A bấm "Liên kết người ấy" và nhận một mã ngắn, ví dụ `MEO-4827`, cùng một link. **Mã hết hạn sau 24 giờ** và chỉ dùng được một lần.
- B nhập mã hoặc mở link là hai tài khoản thành một cặp.
- Liên kết **giữ nguyên vĩnh viễn** cho đến khi một người bấm hủy. Hai người không phải liên kết lại khi đăng nhập lại hay đổi thiết bị.
- Mỗi người chỉ thuộc tối đa **một** cặp tại một thời điểm. Nếu đang thuộc một cặp thì không tạo được mã mời mới và không nhập được mã của người khác.
- **Một người hủy là hủy liên kết.** Ghi chú "Về người ấy" của mỗi bên vẫn được giữ riêng.
- **[Đề xuất]** Khi hủy, các buổi hẹn chưa diễn ra của cặp đó bị hủy theo. Buổi hẹn đã diễn ra vẫn được giữ làm lịch sử của từng người.
- **[Đề xuất]** Khi liên kết, người dùng có thể nhập **ngày bắt đầu yêu nhau**, dùng cho dịp kỷ niệm.

### 6.3 Kho "Về người ấy"
- Gồm các ghi chú tự do về người ấy: thích gì, ghét gì, hay nói gì, dị ứng gì. Có thêm trường riêng cho **ngày sinh** của người ấy.
- Ghi chú được phân tích bằng cùng từ điển từ khóa ở mục 7.2 để rút ra **thẻ sở thích** (thư giãn, handmade, đồ ngọt, ngoài trời…). Thẻ này dùng cho cả gợi ý địa điểm lẫn gợi ý quà.
- Kho này dùng được **cả khi chưa liên kết**, ví dụ với người mới quen chưa dùng app.
- **Chỉ chủ của ghi chú thấy được.** Không có đường nào trên API trả ghi chú này cho người còn lại.

### 6.4 Kho "Người kia muốn gì" (mong muốn)
- Mỗi mong muốn là **một dòng chữ tự do, không theo form**. Ví dụ: *"Dạo này mệt quá, rất muốn có hộp nến thơm tầm 300k"*.
- Lưu ngay, **không hiện phần xem trước** kết quả phân tích.
- Hệ thống phân tích câu để rút ra: loại quà, các thẻ, giá ước tính, mức độ "rất muốn" (xem 7.2).
- Người dùng sửa hoặc xóa mong muốn bất cứ lúc nào. Khi sửa, câu được phân tích lại.
- Câu không nhận ra món quà nào vẫn được lưu nguyên văn, chỉ không tham gia chấm điểm.
- Mong muốn chỉ được dùng để gợi ý cho **người kia trong cặp hiện tại**. Khi chưa liên kết, mong muốn chỉ được lưu và chưa dùng vào việc gì.

### 6.5 Buổi hẹn
Một buổi hẹn gồm:
- ngày giờ;
- **ngân sách đi chơi**, dùng để lọc địa điểm;
- **ngân sách quà**, dùng để gợi ý quà cho người tạo buổi hẹn;
- chế độ: **cùng chọn** hoặc **bất ngờ**.

**Chế độ cùng chọn** (bắt buộc đã liên kết): xem 6.6.

**Chế độ bất ngờ:**
- Người lên kế hoạch xem gợi ý địa điểm (mục 7.1) rồi tự chốt.
- **[Đề xuất]** Người kia **không thấy gì** về buổi hẹn bất ngờ, kể cả ngày giờ. Muốn báo trước thì người lên kế hoạch tự báo bên ngoài app.

**Ngân sách quà là của từng người:** nếu cả hai cùng muốn tặng quà trong một buổi hẹn, mỗi người nhập ngân sách quà riêng. Ngân sách của người này không bao giờ hiện cho người kia.

### 6.6 Chọn địa điểm trong khung chat (chế độ cùng chọn)
- Mỗi buổi hẹn có một khung chat **chỉ dùng để chọn địa điểm**, không phải chat tự do.
- Trong khung chat có:
  - các **thẻ địa điểm** (ảnh, tên, quận, khoảng giá);
  - **thông báo của hệ thống**;
  - một bộ **câu nhắn nhanh có sẵn**, ví dụ "Chỗ này xa quá", "Đắt quá nè", "Đi chỗ khác nha".
- Cả hai đều thêm được địa điểm vào khung chat. Có thể chọn từ gợi ý của hệ thống (7.1), tìm trong kho, hoặc **tự nhập** một địa điểm ngoài kho (tên và địa chỉ dạng chữ).
- Luồng chọn:
  1. Người A bấm **"Chọn chỗ này"** trên một thẻ. Khung chat báo "Đang chờ người ấy…".
  2. Phía người B, **toàn màn hình mờ đi** và thẻ đó **nảy lên giữa màn hình**, có viền sáng và dòng "Người ấy muốn đi chỗ này 💕", kèm hai nút **✕ Không** và **♥ Chọn**.
  3. B bấm ♥: hiệu ứng **"Chốt kèo! 🎉"** với tim bay lên, giống lúc match trên Tinder. Địa điểm được chốt cho buổi hẹn và cả hai đều thấy.
  4. B bấm ✕: khung chat báo "Người ấy muốn xem chỗ khác", và cả hai lại chọn được.
- Mỗi lúc chỉ có **một lựa chọn đang chờ** trong một buổi hẹn. Trong lúc chờ, nút "Chọn" bị khóa.
- Nếu B đang không mở app, lựa chọn được lưu ở trạng thái chờ. Màn hình mờ và thẻ nảy lên sẽ xuất hiện **khi B mở buổi hẹn**.
- **[Đề xuất]** A được hủy lựa chọn mình đang chờ.
- **[Đề xuất]** Sau khi chốt, hai người vẫn có thể "chọn lại", tức là bắt đầu một vòng chọn mới. Chỗ đã chốt chỉ bị thay khi vòng mới được chốt.
- Bản mockup tham khảo (vẽ trong cuộc trao đổi): khung điện thoại; thanh trên cùng ghi tên người ấy và "Buổi hẹn tối thứ 7 · ngân sách 500k"; các thẻ địa điểm xếp ngang và cuộn được.

### 6.7 Chú mèo pixel và câu "Bạn có biết?"
- **Hình dáng:** mèo pixel màu kem, mắt kiểu anime, phong cách pixel art giống hình tham khảo mà chủ dự án đưa.
  - Ló ra **chéo từ mép trái màn hình**, ở **nửa trên** của mép, nghiêng khoảng 25°, thấy gần hết mặt.
  - Bàn chân **giơ ngón trỏ lên** kiểu ☝️, đặt cạnh mặt.
  - **Kích thước nhỏ**, khoảng 96px chiều ngang trên điện thoại, để không che nhiều màn hình.
- **Khung nói:** hình bầu dục viền đậm, có đuôi chỉ về phía mèo. Dòng đầu luôn là **"Bạn có biết?"**, bên dưới là gợi ý. Ví dụ: *"Người ấy từng nói thích thư giãn. Một hộp nến thơm tầm 300k là vừa đẹp!"*
- Hình vẽ cuối cùng do họa sĩ pixel làm, có vài khung hình động (ló ra, giơ tay). Mockup trong cuộc trao đổi chỉ để tham khảo bố cục.
- **Khi nào mèo xuất hiện:** khi người tặng mở app trong **khoảng thời gian trước một dịp tặng quà** (mục 7.3) và có gợi ý cho dịp đó.
- **[Đề xuất]** Mèo hiện ở trang chủ và trang buổi hẹn. Bấm vào khung nói thì khung thu gọn lại. Có nút "Gợi ý khác" để lấy món có điểm cao tiếp theo.

### 6.8 Trang quản trị
Chỉ người dùng có vai trò `admin` mới vào được. Trang quản trị cho phép:
- **Thêm, sửa, ẩn địa điểm.** Mỗi địa điểm có:
  - tên, địa chỉ, quận, tọa độ;
  - khoảng giá mỗi người (tối thiểu và tối đa, VND);
  - điểm đánh giá;
  - ảnh;
  - thẻ (lãng mạn, yên tĩnh, rooftop, ngoài trời, workshop…);
  - giờ mở cửa;
  - loại hình (quán ăn, cà phê, hoạt động…).
- **Quản lý kho ý tưởng quà:** tên, khoảng giá, thẻ, dịp phù hợp.
- **Quản lý từ điển từ khóa** (7.2) và xem **danh sách từ khóa chưa nhận ra**.
- **[Đề xuất]** Điểm đánh giá do quản trị viên nhập, thang 1-5, có thể tham khảo Google Maps. Tọa độ do quản trị viên dán vào. Có thể thêm nút đổi địa chỉ thành tọa độ bằng Goong hoặc Nominatim của OpenStreetMap, nhưng không bắt buộc.
- Bản đầu có khoảng **50-100 địa điểm** ở các quận trung tâm Sài Gòn.

## 7. Luật gợi ý

### 7.1 Gợi ý địa điểm
**Đầu vào:**
- ngân sách đi chơi;
- vị trí của hai người;
- thẻ sở thích rút ra từ "Về người ấy" của người đang xem;
- thời gian buổi hẹn.

**Các bước:**
1. **Điểm giữa:** lấy trung điểm tọa độ của hai người.
   - **[Đề xuất]** Vị trí của chính người đang xem lấy từ định vị của trình duyệt lúc lập kế hoạch. Nếu người dùng không cho phép định vị thì cho nhập khu vực hoặc quận.
   - **[Đề xuất]** Vị trí của người kia trong chế độ cùng chọn lấy từ định vị khi người kia mở buổi hẹn. Trong chế độ bất ngờ, người lên kế hoạch nhập khu vực hoặc quận của người ấy. Không hỏi người ấy, để giữ bất ngờ.
   - Vị trí **chỉ dùng để gợi ý địa điểm đi chơi**.
   - **[Đề xuất]** Không lưu lịch sử vị trí. Chỉ giữ vị trí gần nhất gắn với buổi hẹn.
2. **Lọc theo bán kính** quanh điểm giữa.
   - **[Đề xuất]** Bán kính mặc định 5km. Nếu tìm được ít hơn 3 kết quả thì nới lên 10km.
   - **[Đề xuất]** Tính khoảng cách bằng công thức haversine trên cột lat/lng thường, không cần PostGIS ở quy mô này.
3. **Lọc theo ngân sách:** chỉ giữ địa điểm có `giá tối thiểu mỗi người × 2 ≤ ngân sách đi chơi`.
4. **Lọc giờ mở cửa** theo thời gian buổi hẹn, nếu địa điểm có nhập giờ mở cửa.
5. **Xếp hạng**, ưu tiên theo thứ tự:
   - điểm đánh giá cao (giữ ngưỡng ≥ 4.0 khi có đủ kết quả);
   - khớp nhiều thẻ sở thích;
   - gần điểm giữa;
   - giá sát ngân sách.
   - **[Đề xuất]** Cụ thể là cộng điểm có trọng số, với trọng số được cấu hình được.

### 7.2 Phân tích câu tự do bằng từ điển từ khóa
Áp dụng cho cả mong muốn lẫn ghi chú "Về người ấy". Đây là một domain service thuần, không phụ thuộc hạ tầng nên dễ viết test.

1. **Chuẩn hóa:** chuyển sang chữ thường, tạo thêm bản không dấu, đổi teencode theo bảng quy đổi (mún→muốn, dc→được…).
2. **Dò từ điển.** Từ điển có 4 nhóm:

| Nhóm | Ví dụ | Rút ra |
|---|---|---|
| Món / loại quà | nến thơm, tinh dầu, son, hoa, gấu bông, sách | loại quà và các thẻ đi kèm |
| Ngữ cảnh / sở thích | mệt, stress, thư giãn, handmade, đọc sách | thêm thẻ |
| Mức độ muốn | rất muốn, ước gì, mê, thèm | đánh dấu "rất muốn" |
| Giá | 300k, 300 nghìn, 1tr, 1 triệu rưỡi, dưới 500k | giá ước tính, đọc bằng mẫu nhận dạng số tiền |

3. **Phủ định:** nếu có từ phủ định (không, chẳng, đừng, ghét) đứng ngay trước từ khóa thì từ khóa đó **không** được tính là mong muốn. Trong "Về người ấy", từ khóa đó được ghi thành thẻ "không thích".
4. **Một câu nhiều món** ("muốn son hoặc nước hoa") được tách thành nhiều mong muốn con.
5. **Thiếu giá:** dùng khoảng giá trung bình của loại quà đó trong kho ý tưởng quà.
6. Những từ có nghĩa nhưng không có trong từ điển được ghi vào **danh sách từ khóa chưa nhận ra** để quản trị viên bổ sung dần.
   - **[Đề xuất]** Để bảo vệ quyền riêng tư, danh sách này chỉ lưu **từng từ hoặc cụm 1-3 từ riêng lẻ, kèm số lần xuất hiện**. Không lưu câu gốc, không lưu ngữ cảnh xung quanh, không gắn với người dùng nào. Quản trị viên không bao giờ thấy được nội dung câu riêng tư của người dùng.

- **[Đề xuất]** Từ điển lưu trong cơ sở dữ liệu để quản trị viên sửa được mà không phải deploy lại. Từ điển được nạp vào bộ nhớ lúc khởi động và nạp lại khi quản trị viên sửa.
- Từ điển ban đầu cần **vài trăm từ khóa** cho các loại quà phổ biến.

### 7.3 Dịp tặng quà và lúc mèo xuất hiện
**Các dịp:**
- **Buổi hẹn** có ngày giờ.
- **Ngày lễ của cặp đôi Việt Nam:**
  - 14/2 (Valentine);
  - 14/3 (Valentine Trắng);
  - 8/3;
  - 20/10;
  - 24/12 và 25/12 (Giáng sinh);
  - **Thất tịch** (ngày 7/7 âm lịch).
- **Sinh nhật người ấy**, lấy từ kho "Về người ấy".
- **[Đề xuất]** **Ngày kỷ niệm yêu nhau**, mỗi năm một lần.

**Quy tắc:**
- **[Đề xuất]** Thất tịch dùng **bảng ngày dương lịch nhập sẵn** cho từng năm, ví dụ 2026 đến 2035, thay vì tự viết thuật toán đổi lịch âm.
- Mèo xuất hiện khi người tặng **mở app** trong khoảng thời gian trước dịp. Không có thông báo đẩy và không có email nhắc.
- **[Đề xuất]** Khoảng thời gian đó là **3 ngày** trước dịp, kéo dài đến hết ngày diễn ra dịp.
- **[Đề xuất]** Với ngày lễ, sinh nhật và kỷ niệm (không có buổi hẹn kèm ngân sách), mèo hỏi ngân sách quà ngay trong khung nói trước khi đưa gợi ý. Ngân sách này được lưu cho dịp đó.
- **[Đề xuất]** Không cần tác vụ chạy nền theo lịch. Hệ thống tính dịp sắp tới ngay khi người dùng mở app. Như vậy không phải làm thêm phần lập lịch.

### 7.4 Chọn quà
**Đầu vào:**
- ngân sách quà;
- dịp tặng;
- mong muốn của người nhận (chỉ có khi đã liên kết);
- thẻ rút ra từ "Về người ấy" của người tặng.

**Các bước:**
1. **Loại món vượt ngân sách.** Chỉ giữ mong muốn có giá ước tính ≤ ngân sách quà.
2. **Chấm điểm** những món còn lại. Trọng số mặc định dưới đây cấu hình được:

| Tiêu chí | Điểm |
|---|---|
| Người nhận ghi "rất muốn" | +3 |
| Mỗi thẻ trùng với "Về người ấy" | +2 |
| Hợp với dịp (thẻ dịp của loại quà) | +1 |
| Giá ≥ 80% ngân sách | +1 |
| Đã được gợi ý ở dịp trước | −3 |
| Trùng thẻ "không thích" | loại luôn |

3. **Chọn món có điểm cao nhất.** Nếu bằng điểm, chọn món có giá gần ngân sách hơn.
4. **Dự phòng:** nếu chưa liên kết, hoặc không mong muốn nào qua được bước 1, thì chấm điểm trên **kho ý tưởng quà** theo cùng cách, chỉ bỏ tiêu chí "rất muốn".
5. **Câu thoại:** ghép từ mẫu câu có sẵn với lý do chính khiến món đó được nhiều điểm. Ví dụ: "Người ấy từng nói thích {thẻ}. {Món} tầm {giá} là vừa đẹp!"

**Giữ bất ngờ:**
- Người tặng thấy món được gợi ý và lý do, nhưng **không thấy câu gốc** của người nhận.
  - **[Đề xuất]** Câu thoại chỉ dùng tên món và thẻ, không trích nguyên văn câu người nhận đã ghi.
- Người nhận **không thấy** mong muốn nào của mình đã được gợi ý. Lịch sử "đã gợi ý" chỉ dùng nội bộ cho tiêu chí −3.

## 8. Kiến trúc (khung)

### Backend
Modular monolith theo Clean Architecture, cấu trúc như trong `.claude/CLAUDE.md`. **[Đề xuất]** Gồm các module sau:

| Module | Trách nhiệm |
|---|---|
| `auth` | đăng ký, đăng nhập, JWT, refresh token, đặt lại mật khẩu |
| `users` | hồ sơ người dùng, vai trò |
| `couples` | mã mời, liên kết, hủy liên kết, ngày kỷ niệm |
| `partner_notes` | kho "Về người ấy" |
| `wishes` | kho mong muốn |
| `text_analysis` | từ điển, bộ phân tích câu (7.2), danh sách từ chưa nhận ra |
| `places` | kho địa điểm, gợi ý địa điểm (7.1) |
| `date_plans` | buổi hẹn, khung chat chọn địa điểm, lựa chọn đang chờ, WebSocket |
| `gifts` | kho ý tưởng quà, dịp tặng (7.3), chọn quà (7.4), lịch sử gợi ý |
| `admin` | các API quản trị, chỉ gom route và kiểm tra quyền; logic nằm trong các module trên |

**Cập nhật tức thì:**
- Dùng WebSocket sẵn có của Axum cho khung chat chọn địa điểm.
- Xác thực bằng JWT lúc mở kết nối, và kiểm tra người dùng thuộc cặp của buổi hẹn đó.
- **[Đề xuất]** Kết nối được quản lý trong bộ nhớ của tiến trình bằng `tokio::sync::broadcast` theo từng buổi hẹn. Không dùng Redis, vì chạy một instance là đủ.
- Trạng thái thật luôn nằm trong PostgreSQL. Khi kết nối lại, client tải lại toàn bộ trạng thái của buổi hẹn.

**Phân quyền** phải được kiểm tra ở tầng application, không chỉ ở middleware:
- ghi chú "Về người ấy" chỉ chủ ghi chú xem được;
- mong muốn chỉ chủ mong muốn xem được;
- buổi hẹn bất ngờ chỉ người tạo xem được;
- API quản trị chỉ `admin` gọi được.

### Frontend
- React, Vite và TypeScript, ưu tiên màn hình điện thoại.
- Trang quản trị nằm trong cùng ứng dụng, dưới đường dẫn `/admin`.
- Sprite mèo được vẽ ở chế độ pixel sắc nét (`image-rendering: pixelated`).

### Hạ tầng
- Dùng Docker Compose: backend, PostgreSQL, Mailpit (môi trường phát triển).
- **[Đề xuất]** Ảnh địa điểm lưu ở thư mục được backend phục vụ trực tiếp trong bản đầu. Chuyển sang object storage khi triển khai thật.
- **[Đề xuất]** Frontend và API chạy **cùng một origin**. Một reverse proxy (ví dụ Caddy hoặc Nginx) phục vụ bản build của frontend và chuyển `/api` cùng WebSocket sang backend. Khi đó cookie refresh token dùng `SameSite=Strict` và không cần cấu hình CORS. Khi phát triển, Vite proxy `/api` sang backend.

## 9. Để lại cho người xây dựng
- Tên bảng, cột, index và constraint cụ thể.
- Hình dạng request và response của từng endpoint, cùng tài liệu OpenAPI bằng Utoipa.
- Cấu trúc thư mục và component của frontend, thư viện quản lý state, thư viện router.
- Bộ câu nhắn nhanh trong khung chat và bộ mẫu câu thoại của mèo.
- Nội dung ban đầu của từ điển từ khóa và kho ý tưởng quà. Chủ dự án hoặc quản trị viên sẽ soạn, người xây dựng chỉ cần seed một bộ nhỏ để test.
- Hiệu ứng chuyển động chi tiết (thời gian, đường cong easing).

## 10. Chưa biết

Chủ dự án chưa nói rõ thế nào là thành công (số người dùng, bao nhiêu cặp quay lại…). Điều này không ảnh hưởng đến cách xây bản đầu tiên.
