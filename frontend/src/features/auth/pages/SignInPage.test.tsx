import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { refreshAccessToken } from '../../../shared/api/client';
import { ApiError } from '../../../shared/api/errors';
import { getAccessToken, setAccessToken } from '../../../shared/api/tokenStore';
import { authResponse } from '../../../test/fixtures';
import { currentLocation, renderApp } from '../../../test/renderApp';
import { login } from '../api';

vi.mock('../api', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
}));

vi.mock('../../../shared/api/client', () => ({
  refreshAccessToken: vi.fn(),
  onSessionExpired: vi.fn(() => () => {}),
  waitForPendingRefresh: vi.fn(() => Promise.resolve()),
}));

beforeEach(() => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  setAccessToken(null);
});

afterEach(() => {
  vi.clearAllMocks();
});

async function fillAndSubmit(email: string, password: string) {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('Email'), email);
  await user.type(screen.getByLabelText('Mật khẩu'), password);
  await user.click(screen.getByRole('button', { name: 'Đăng nhập' }));
}

test('SignInPage shows password length error without calling the api', async () => {
  renderApp('/sign-in');
  // Seven emoji are 14 UTF-16 units but only 7 characters: still too short.
  await fillAndSubmit('an@example.com', '\u{1F600}'.repeat(7));

  expect(
    await screen.findByText('Mật khẩu cần ít nhất 8 ký tự.'),
  ).toBeInTheDocument();
  expect(screen.getByLabelText('Mật khẩu')).toHaveAttribute(
    'aria-invalid',
    'true',
  );
  expect(login).not.toHaveBeenCalled();
});

test('SignInPage signs in and navigates to the page the user came from', async () => {
  vi.mocked(login).mockResolvedValue(authResponse('an@example.com', 'tok-1'));
  renderApp({
    pathname: '/sign-in',
    state: { from: { pathname: '/settings', search: '?tab=gender' } },
  });
  await fillAndSubmit('an@example.com', 'mat-khau-dai');

  await waitFor(() => {
    expect(currentLocation()).toBe('/settings?tab=gender');
  });
  expect(login).toHaveBeenCalledWith('an@example.com', 'mat-khau-dai');
  expect(getAccessToken()).toBe('tok-1');
});

test('SignInPage ignores a redirect target outside the app', async () => {
  vi.mocked(login).mockResolvedValue(authResponse('an@example.com'));
  renderApp({
    pathname: '/sign-in',
    state: { from: { pathname: '//evil.example', search: '' } },
  });
  await fillAndSubmit('an@example.com', 'mat-khau-dai');

  expect(
    await screen.findByRole('heading', { name: 'Xin chào, an@example.com' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/');
});

test('SignInPage shows invalid_credentials message', async () => {
  vi.mocked(login).mockRejectedValue(new ApiError(401, 'invalid_credentials'));
  renderApp('/sign-in');
  await fillAndSubmit('an@example.com', 'sai-mat-khau');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Email hoặc mật khẩu chưa đúng.',
  );
  expect(screen.getByRole('button', { name: 'Đăng nhập' })).toBeEnabled();
});

test('SignInPage shows rate_limited message with minutes from Retry-After', async () => {
  vi.mocked(login).mockRejectedValue(
    new ApiError(429, 'rate_limited', undefined, { retryAfterSecs: 900 }),
  );
  renderApp('/sign-in');
  await fillAndSubmit('an@example.com', 'mat-khau-dai');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Bạn thử nhiều lần quá. Đợi khoảng 15 phút rồi thử lại nhé.',
  );
});

test('SignInPage disables submit while the request is pending', async () => {
  vi.mocked(login).mockReturnValue(new Promise(() => {}));
  renderApp('/sign-in');
  await fillAndSubmit('an@example.com', 'mat-khau-dai');

  expect(screen.getByRole('button', { name: 'Đăng nhập' })).toBeDisabled();
});

test('SignInPage redirects a signed-in user to home', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(authResponse());
  renderApp('/sign-in');

  expect(
    await screen.findByRole('heading', { name: 'Xin chào, an@example.com' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/');
});

test('SignInPage labels its fields and links to password reset and sign-up', async () => {
  renderApp('/sign-in');

  expect(
    await screen.findByRole('link', { name: 'Quên mật khẩu?' }),
  ).toHaveAttribute('href', '/forgot-password');
  expect(screen.getByRole('link', { name: 'Đăng ký' })).toHaveAttribute(
    'href',
    '/sign-up',
  );
  expect(screen.getByLabelText('Email')).toHaveAttribute(
    'autocomplete',
    'email',
  );
  expect(screen.getByLabelText('Mật khẩu')).toHaveAttribute(
    'autocomplete',
    'current-password',
  );
});
