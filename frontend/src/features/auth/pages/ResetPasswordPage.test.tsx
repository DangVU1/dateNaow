import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { refreshAccessToken } from '../../../shared/api/client';
import { ApiError } from '../../../shared/api/errors';
import { getAccessToken, setAccessToken } from '../../../shared/api/tokenStore';
import { authResponse } from '../../../test/fixtures';
import { renderApp } from '../../../test/renderApp';
import { confirmPasswordReset, logout } from '../api';

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

const EXPIRED = 'Link này đã hết hạn hoặc đã được dùng. Bạn xin link mới nhé.';
const SUCCESS = 'Đổi mật khẩu xong. Mời bạn đăng nhập lại.';

/**
 * The page reads the real address bar (`window.location`), as the emailed
 * link opens it; the in-memory router only has to be on the same path.
 */
function openResetLink(url: string) {
  window.history.replaceState(null, '', url);
  const { pathname, search } = window.location;
  renderApp(pathname + search, { strictMode: true });
}

async function fillAndSubmit(password: string, confirm: string) {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('Mật khẩu mới'), password);
  await user.type(screen.getByLabelText('Nhập lại mật khẩu'), confirm);
  await user.click(screen.getByRole('button', { name: 'Đổi mật khẩu' }));
}

beforeEach(() => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  vi.mocked(logout).mockResolvedValue(undefined);
  setAccessToken(null);
});

afterEach(() => {
  window.history.replaceState(null, '', '/');
  vi.clearAllMocks();
});

test('ResetPasswordPage reads the token from the hash, strips it, and submits', async () => {
  vi.mocked(confirmPasswordReset).mockResolvedValue(undefined);
  openResetLink('/reset-password?from=email#token=abc123');

  expect(await screen.findByLabelText('Mật khẩu mới')).toHaveAttribute(
    'autocomplete',
    'new-password',
  );
  expect(screen.getByLabelText('Nhập lại mật khẩu')).toHaveAttribute(
    'autocomplete',
    'new-password',
  );
  // Gone from the address bar; path and query are kept.
  expect(window.location.hash).toBe('');
  expect(window.location.pathname + window.location.search).toBe(
    '/reset-password?from=email',
  );
  expect(document.body.textContent).not.toContain('abc123');

  await fillAndSubmit('mật khẩu mới', 'mật khẩu mới');

  expect(await screen.findByRole('status')).toHaveTextContent(SUCCESS);
  expect(confirmPasswordReset).toHaveBeenCalledTimes(1);
  expect(confirmPasswordReset).toHaveBeenCalledWith('abc123', 'mật khẩu mới');
  expect(screen.getByRole('link', { name: 'Đăng nhập' })).toHaveAttribute(
    'href',
    '/sign-in',
  );
});

test('ResetPasswordPage shows the expired message on invalid_or_expired_token', async () => {
  vi.mocked(confirmPasswordReset).mockRejectedValue(
    new ApiError(400, 'invalid_or_expired_token'),
  );
  openResetLink('/reset-password#token=used-token');
  await fillAndSubmit('mat-khau-dai', 'mat-khau-dai');

  expect(await screen.findByRole('alert')).toHaveTextContent(EXPIRED);
  expect(screen.getByRole('link', { name: 'Xin link mới' })).toHaveAttribute(
    'href',
    '/forgot-password',
  );
  expect(screen.queryByLabelText('Mật khẩu mới')).not.toBeInTheDocument();
});

test('ResetPasswordPage shows the expired message at once when there is no token', async () => {
  openResetLink('/reset-password');

  expect(await screen.findByRole('alert')).toHaveTextContent(EXPIRED);
  expect(screen.queryByLabelText('Mật khẩu mới')).not.toBeInTheDocument();
  expect(confirmPasswordReset).not.toHaveBeenCalled();
});

test('ResetPasswordPage rejects mismatched passwords before calling the api', async () => {
  openResetLink('/reset-password#token=abc123');
  await fillAndSubmit('mat-khau-dai', 'mat-khau-khac');

  expect(
    await screen.findByText('Hai mật khẩu chưa khớp nhau.'),
  ).toBeInTheDocument();
  expect(screen.getByLabelText('Nhập lại mật khẩu')).toHaveAttribute(
    'aria-invalid',
    'true',
  );
  expect(confirmPasswordReset).not.toHaveBeenCalled();
});

test('ResetPasswordPage shows a server 422 for new_password on its field', async () => {
  vi.mocked(confirmPasswordReset).mockRejectedValue(
    new ApiError(422, 'validation_error', undefined, {
      fields: { new_password: ['too_long'] },
    }),
  );
  openResetLink('/reset-password#token=abc123');
  await fillAndSubmit('mat-khau-dai', 'mat-khau-dai');

  expect(
    await screen.findByText('Mật khẩu dài tối đa 128 ký tự.'),
  ).toBeInTheDocument();
  expect(screen.getByLabelText('Mật khẩu mới')).toHaveAttribute(
    'aria-invalid',
    'true',
  );
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

test('ResetPasswordPage signs a signed-in user out locally after a reset', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(
    authResponse('an@example.com', 'tok-1'),
  );
  setAccessToken('tok-1');
  vi.mocked(confirmPasswordReset).mockResolvedValue(undefined);
  openResetLink('/reset-password#token=abc123');
  await fillAndSubmit('mat-khau-dai', 'mat-khau-dai');

  expect(await screen.findByRole('status')).toHaveTextContent(SUCCESS);
  await waitFor(() => {
    expect(logout).toHaveBeenCalledTimes(1);
  });
  await waitFor(() => {
    expect(getAccessToken()).toBeNull();
  });
  expect(screen.getByRole('status')).toHaveTextContent(SUCCESS);
});
