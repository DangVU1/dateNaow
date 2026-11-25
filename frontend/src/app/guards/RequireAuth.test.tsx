import { screen } from '@testing-library/react';
import { refreshAccessToken } from '../../shared/api/client';
import { setAccessToken } from '../../shared/api/tokenStore';
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

test('RequireAuth shows loading status while the session restores', () => {
  vi.mocked(refreshAccessToken).mockReturnValue(new Promise(() => {}));
  renderApp('/');

  expect(screen.getByRole('status')).toHaveTextContent('Đang tải…');
  expect(
    screen.queryByRole('heading', { name: /Xin chào/ }),
  ).not.toBeInTheDocument();
});

test('RequireAuth redirects anonymous users to /sign-in', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  renderApp('/');

  expect(
    await screen.findByRole('heading', { name: 'Đăng nhập' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/sign-in');
});

test('unknown routes stay reachable without signing in', async () => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  renderApp('/khong-co');

  expect(
    await screen.findByRole('heading', { name: 'Không tìm thấy trang' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/khong-co');
});
