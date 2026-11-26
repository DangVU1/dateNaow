import { ApiError } from '../../shared/api/errors';

const DEFAULT_RATE_LIMIT_MINUTES = 15;
const GENERIC_MESSAGE = 'Đã có lỗi, bạn thử lại sau nhé.';

/** Vietnamese text for an API error code (P10). */
export function errorMessage(err: ApiError): string {
  switch (err.code) {
    case 'invalid_credentials':
      return 'Email hoặc mật khẩu chưa đúng.';
    case 'email_taken':
      return 'Email này đã có tài khoản. Bạn đăng nhập nhé.';
    case 'rate_limited': {
      const minutes =
        err.retryAfterSecs === undefined
          ? DEFAULT_RATE_LIMIT_MINUTES
          : Math.max(1, Math.ceil(err.retryAfterSecs / 60));
      return `Bạn thử nhiều lần quá. Đợi khoảng ${minutes} phút rồi thử lại nhé.`;
    }
    case 'network_error':
      return 'Không kết nối được. Bạn kiểm tra mạng rồi thử lại nhé.';
    default:
      return GENERIC_MESSAGE;
  }
}

/** Like `errorMessage`, for anything a submit handler may catch. */
export function anyErrorMessage(err: unknown): string {
  return err instanceof ApiError ? errorMessage(err) : GENERIC_MESSAGE;
}

/** Vietnamese text for a 422 field error, by field name and code. */
export function fieldMessage(field: string, code: string): string {
  if (field === 'email') {
    if (code === 'required') return 'Bạn nhập email nhé.';
    if (code === 'invalid_email') return 'Email chưa đúng định dạng.';
    if (code === 'too_long') return 'Email dài quá.';
  }
  if (field === 'password' || field === 'new_password') {
    if (code === 'too_short') return 'Mật khẩu cần ít nhất 8 ký tự.';
    if (code === 'too_long') return 'Mật khẩu dài tối đa 128 ký tự.';
    // Only the backend knows the common-password list (DECISIONS #16).
    if (code === 'too_common') {
      return 'Mật khẩu này quá phổ biến, dễ bị đoán. Bạn chọn mật khẩu khác nhé.';
    }
  }
  if (field === 'confirm_password' && code === 'mismatch') {
    return 'Hai mật khẩu chưa khớp nhau.';
  }
  return 'Giá trị chưa hợp lệ.';
}
