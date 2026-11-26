import { fieldMessage } from './messages';

const TOO_COMMON =
  'Mật khẩu này quá phổ biến, dễ bị đoán. Bạn chọn mật khẩu khác nhé.';

test('fieldMessage explains too_common for password and new_password', () => {
  expect(fieldMessage('password', 'too_common')).toBe(TOO_COMMON);
  expect(fieldMessage('new_password', 'too_common')).toBe(TOO_COMMON);
});

test('fieldMessage keeps the generic text for too_common on other fields', () => {
  expect(fieldMessage('email', 'too_common')).toBe('Giá trị chưa hợp lệ.');
});
