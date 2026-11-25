import { act, cleanup, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { refreshAccessToken } from '../../../shared/api/client';
import { ApiError } from '../../../shared/api/errors';
import { setAccessToken } from '../../../shared/api/tokenStore';
import { authResponse } from '../../../test/fixtures';
import { currentLocation, renderApp } from '../../../test/renderApp';
import { requestPasswordReset } from '../api';

vi.mock('../api', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
  requestPasswordReset: vi.fn(),
  confirmPasswordReset: vi.fn(),
}));

vi.mock('../../../shared/api/client', () => ({
  refreshAccessToken: vi.fn(),
  onSessionExpired: vi.fn(() => () => {}),
  waitForPendingRefresh: vi.fn(() => Promise.resolve()),
}));

const CONFIRMATION =
  'Nếu email này có tài khoản, bạn sẽ nhận được link đặt lại mật khẩu trong vài phút.';

beforeEach(() => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  setAccessToken(null);
});

afterEach(() => {
  vi.clearAllMocks();
});

async function submitEmail(email: string) {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('Email'), email);
  await user.click(screen.getByRole('button', { name: 'Gửi link đặt lại' }));
}

test('ForgotPasswordPage shows the same confirmation whether or not the email exists', async () => {
  // The backend answers 204 for both; the page must not tell them apart.
  vi.mocked(requestPasswordReset).mockResolvedValue(undefined);

  renderApp('/forgot-password');
  await submitEmail('co-tai-khoan@example.com');
  expect(await screen.findByRole('status')).toHaveTextContent(CONFIRMATION);
  const first = document.body.textContent;
  cleanup();

  renderApp('/forgot-password');
  await submitEmail('khong-ai-dung@example.com');
  expect(await screen.findByRole('status')).toHaveTextContent(CONFIRMATION);
  expect(document.body.textContent).toBe(first);

  expect(requestPasswordReset).toHaveBeenNthCalledWith(
    1,
    'co-tai-khoan@example.com',
  );
  expect(requestPasswordReset).toHaveBeenNthCalledWith(
    2,
    'khong-ai-dung@example.com',
  );
  expect(
    screen.getByRole('link', { name: 'Về trang đăng nhập' }),
  ).toHaveAttribute('href', '/sign-in');
});

test('ForgotPasswordPage shows 60 minutes when rate limited with Retry-After 3600', async () => {
  vi.mocked(requestPasswordReset).mockRejectedValue(
    new ApiError(429, 'rate_limited', undefined, { retryAfterSecs: 3600 }),
  );
  renderApp('/forgot-password');
  await submitEmail('an@example.com');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Bạn thử nhiều lần quá. Đợi khoảng 60 phút rồi thử lại nhé.',
  );
  expect(screen.queryByText(CONFIRMATION)).not.toBeInTheDocument();
  expect(
    screen.getByRole('button', { name: 'Gửi link đặt lại' }),
  ).toBeEnabled();
});

test('ForgotPasswordPage asks for an email without calling the api', async () => {
  renderApp('/forgot-password');
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole('button', { name: 'Gửi link đặt lại' }),
  );

  expect(await screen.findByText('Bạn nhập email nhé.')).toBeInTheDocument();
  expect(requestPasswordReset).not.toHaveBeenCalled();
});

test('ForgotPasswordPage stays reachable for a signed-in user', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(authResponse());
  renderApp('/forgot-password');
  // Let the session restore settle so a guard would have redirected by now.
  await act(async () => {
    await Promise.resolve();
  });

  expect(
    screen.getByRole('heading', { name: 'Quên mật khẩu' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/forgot-password');
});

test('ForgotPasswordPage shows the network message instead of the confirmation when nothing was sent', async () => {
  vi.mocked(requestPasswordReset).mockRejectedValue(
    new ApiError(0, 'network_error'),
  );
  renderApp('/forgot-password');

  await submitEmail('an@example.com');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Không kết nối được. Bạn kiểm tra mạng rồi thử lại nhé.',
  );
  expect(screen.queryByText(CONFIRMATION)).not.toBeInTheDocument();
});
