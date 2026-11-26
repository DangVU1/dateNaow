import { screen } from '@testing-library/react';
import { refreshAccessToken } from '../../shared/api/client';
import { setAccessToken } from '../../shared/api/tokenStore';
import { authResponse } from '../../test/fixtures';
import { currentLocation, renderApp } from '../../test/renderApp';

vi.mock('../../features/auth/api', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
}));

vi.mock('../../shared/api/client', () => ({
  refreshAccessToken: vi.fn(),
  onSessionExpired: vi.fn(() => () => {}),
  waitForPendingRefresh: vi.fn(() => Promise.resolve()),
}));

beforeEach(() => {
  setAccessToken(null);
});

afterEach(() => {
  vi.clearAllMocks();
});

test('RequireAdmin shows forbidden page to a non-admin user', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(
    authResponse('an@example.com', 'tok-1', { role: 'user' }),
  );
  renderApp('/admin');

  expect(
    await screen.findByRole('heading', {
      name: 'Bạn không có quyền vào trang này.',
    }),
  ).toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Về trang chủ' })).toHaveAttribute(
    'href',
    '/',
  );
  expect(
    screen.queryByRole('heading', { name: 'Trang quản trị' }),
  ).not.toBeInTheDocument();
  expect(currentLocation()).toBe('/admin');
});

test('RequireAdmin renders admin home for an admin user', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(
    authResponse('admin@example.com', 'tok-1', { role: 'admin' }),
  );
  renderApp('/admin');

  expect(
    await screen.findByRole('heading', { name: 'Trang quản trị' }),
  ).toBeInTheDocument();
  expect(
    screen.getByText('Các mục quản trị sẽ có ở các bản sau.'),
  ).toBeInTheDocument();
});

test('RequireAdmin sends anonymous users to /sign-in first', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  renderApp('/admin');

  expect(
    await screen.findByRole('heading', { name: 'Đăng nhập' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/sign-in');
});
