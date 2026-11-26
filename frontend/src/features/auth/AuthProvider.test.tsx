// Uses the real API client and token store with a stubbed `fetch`, so the
// provider is tested against the actual refresh and session-expiry logic.
import { act, renderHook, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { apiFetch } from '../../shared/api/client';
import { ApiError } from '../../shared/api/errors';
import { getAccessToken, setAccessToken } from '../../shared/api/tokenStore';
import { authResponse } from '../../test/fixtures';
import { currentLocation, renderApp } from '../../test/renderApp';
import { AppProviders } from '../../app/providers';
import { login, logout } from './api';
import { useAuth, type SignOutResult } from './useAuth';

vi.mock('./api', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
}));

const fetchMock = vi.fn<typeof fetch>();

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function errorResponse(status: number, code: string): Response {
  return jsonResponse(status, { error: { code, message: 'msg' } });
}

function calledUrls(): string[] {
  return fetchMock.mock.calls.map(([input]) =>
    typeof input === 'string'
      ? input
      : input instanceof URL
        ? input.href
        : input.url,
  );
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

async function flush() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 20));
  });
}

beforeEach(() => {
  vi.stubGlobal('fetch', fetchMock);
  vi.mocked(logout).mockResolvedValue(undefined);
  setAccessToken(null);
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
  vi.clearAllMocks();
});

test('AuthProvider restores the session on mount via refresh', async () => {
  fetchMock.mockResolvedValueOnce(
    jsonResponse(200, authResponse('an@example.com', 'tok-a')),
  );
  renderApp('/');

  expect(
    await screen.findByRole('heading', { name: 'Xin chào, an@example.com' }),
  ).toBeInTheDocument();
  expect(calledUrls()).toEqual(['/api/v1/auth/refresh']);
  expect(getAccessToken()).toBe('tok-a');
});

test('AuthProvider sends a single refresh under StrictMode', async () => {
  fetchMock.mockResolvedValue(jsonResponse(200, authResponse()));
  renderApp('/', { strictMode: true });

  expect(
    await screen.findByRole('heading', { name: 'Xin chào, an@example.com' }),
  ).toBeInTheDocument();
  await flush();
  expect(calledUrls()).toEqual(['/api/v1/auth/refresh']);
});

test('session expiry while on home redirects once to /sign-in', async () => {
  fetchMock
    .mockResolvedValueOnce(jsonResponse(200, authResponse()))
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
    .mockResolvedValueOnce(errorResponse(401, 'invalid_refresh_token'));
  renderApp('/');
  await screen.findByRole('heading', { name: 'Xin chào, an@example.com' });

  // A request on home finds both tokens expired.
  await act(async () => {
    await expect(apiFetch('/me')).rejects.toBeInstanceOf(ApiError);
  });

  expect(
    await screen.findByRole('heading', { name: 'Đăng nhập' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/sign-in');
  await flush();
  // Mount refresh, /me, failed refresh: nothing more, so no redirect loop.
  expect(calledUrls()).toEqual([
    '/api/v1/auth/refresh',
    '/api/v1/me',
    '/api/v1/auth/refresh',
  ]);
  expect(currentLocation()).toBe('/sign-in');
  expect(getAccessToken()).toBeNull();
});

test('signOut waits for an in-flight refresh so no token survives it', async () => {
  const lateRefresh = deferred<Response>();
  fetchMock
    .mockResolvedValueOnce(
      jsonResponse(200, authResponse('an@example.com', 'tok-a')),
    )
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
    .mockReturnValueOnce(lateRefresh.promise)
    .mockResolvedValue(jsonResponse(200, { ok: true }));
  renderApp('/');
  await screen.findByRole('heading', { name: 'Xin chào, an@example.com' });

  // A request on home hits 401 and starts a refresh that is still running...
  const pending = apiFetch('/me');
  await flush();
  // ...when the user signs out.
  await userEvent.click(screen.getByRole('button', { name: 'Đăng xuất' }));
  await flush();
  // Logout must carry the newest cookie, so it waits for the refresh.
  expect(logout).not.toHaveBeenCalled();

  await act(async () => {
    lateRefresh.resolve(
      jsonResponse(200, authResponse('an@example.com', 'tok-late')),
    );
    await pending;
  });

  expect(
    await screen.findByRole('heading', { name: 'Đăng nhập' }),
  ).toBeInTheDocument();
  expect(logout).toHaveBeenCalledTimes(1);
  expect(getAccessToken()).toBeNull();
});

const SIGN_OUT_NOT_CONFIRMED =
  "Bạn đã đăng xuất trên máy này, nhưng máy chủ chưa xác nhận. Nếu lo ngại, bạn dùng 'Quên mật khẩu?' để đặt lại mật khẩu và đăng xuất khỏi mọi nơi nhé.";
const SIGN_OUT_NOT_REACHED =
  'Chưa đăng xuất được vì không kết nối được máy chủ. Bạn thử đăng xuất lại nhé.';

test('signOut after a backend 500 clears the session and says the server did not confirm', async () => {
  // DECISIONS #17: the server could not revoke the refresh token, but its
  // 500 response cleared the cookie.
  vi.mocked(logout).mockRejectedValue(new ApiError(500, 'internal_error'));
  fetchMock.mockResolvedValueOnce(
    jsonResponse(200, authResponse('an@example.com', 'tok-a')),
  );
  renderApp('/');
  await screen.findByRole('heading', { name: 'Xin chào, an@example.com' });

  await userEvent.click(screen.getByRole('button', { name: 'Đăng xuất' }));

  expect(
    await screen.findByRole('heading', { name: 'Đăng nhập' }),
  ).toBeInTheDocument();
  expect(getAccessToken()).toBeNull();
  expect(screen.getByRole('alert')).toHaveTextContent(SIGN_OUT_NOT_CONFIRMED);
});

test.each([
  ['a network error', new ApiError(0, 'network_error')],
  // e.g. a proxy's 502: the backend never answered, so the cookie stays.
  ['a non-backend error', new ApiError(502, 'unknown_error')],
])(
  'signOut after %s keeps the session and lets the user retry',
  async (_label, error) => {
    vi.mocked(logout).mockRejectedValueOnce(error);
    fetchMock.mockResolvedValueOnce(
      jsonResponse(200, authResponse('an@example.com', 'tok-a')),
    );
    renderApp('/');
    await screen.findByRole('heading', { name: 'Xin chào, an@example.com' });

    await userEvent.click(screen.getByRole('button', { name: 'Đăng xuất' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      SIGN_OUT_NOT_REACHED,
    );
    expect(
      screen.getByRole('heading', { name: 'Xin chào, an@example.com' }),
    ).toBeInTheDocument();
    expect(getAccessToken()).toBe('tok-a');
    const button = screen.getByRole('button', { name: 'Đăng xuất' });
    expect(button).toBeEnabled();

    // The retry reaches the server and signs out.
    await userEvent.click(button);

    expect(
      await screen.findByRole('heading', { name: 'Đăng nhập' }),
    ).toBeInTheDocument();
    expect(logout).toHaveBeenCalledTimes(2);
    expect(getAccessToken()).toBeNull();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  },
);

test('a confirmed signOut shows no warning', async () => {
  fetchMock.mockResolvedValueOnce(
    jsonResponse(200, authResponse('an@example.com', 'tok-a')),
  );
  renderApp('/');
  await screen.findByRole('heading', { name: 'Xin chào, an@example.com' });

  await userEvent.click(screen.getByRole('button', { name: 'Đăng xuất' }));

  expect(
    await screen.findByRole('heading', { name: 'Đăng nhập' }),
  ).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

test('the sign-out warning goes away once a new session starts', async () => {
  vi.mocked(logout).mockRejectedValue(new ApiError(500, 'internal_error'));
  fetchMock.mockResolvedValueOnce(
    jsonResponse(200, authResponse('an@example.com', 'tok-a')),
  );
  const { result } = renderHook(() => useAuth(), { wrapper: AppProviders });
  await waitFor(() => {
    expect(result.current.status).toBe('authenticated');
  });

  let outcome: SignOutResult | undefined;
  await act(async () => {
    outcome = await result.current.signOut();
  });
  expect(outcome).toBe('not_confirmed');
  expect(result.current.status).toBe('anonymous');
  expect(result.current.signOutFailed).toBe(true);

  vi.mocked(login).mockResolvedValue(authResponse('an@example.com', 'tok-b'));
  await act(async () => {
    await result.current.signIn('an@example.com', 'mat-khau-1');
  });
  expect(result.current.signOutFailed).toBe(false);
});

test('signIn while the mount refresh is pending keeps the new user', async () => {
  const mountRefresh = deferred<Response>();
  fetchMock.mockReturnValueOnce(mountRefresh.promise);
  vi.mocked(login).mockResolvedValue(authResponse('binh@example.com', 'tok-b'));
  const { result } = renderHook(() => useAuth(), { wrapper: AppProviders });

  let signedIn: Promise<void> = Promise.resolve();
  act(() => {
    signedIn = result.current.signIn('binh@example.com', 'mat-khau-1');
  });
  await flush();
  expect(login).not.toHaveBeenCalled();

  await act(async () => {
    mountRefresh.resolve(
      jsonResponse(200, authResponse('an@example.com', 'tok-a')),
    );
    await signedIn;
  });

  expect(getAccessToken()).toBe('tok-b');
  expect(result.current.status).toBe('authenticated');
  expect(result.current.user?.email).toBe('binh@example.com');
});

test('useAuth throws outside AuthProvider', () => {
  const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
  try {
    expect(() => renderHook(() => useAuth())).toThrow(/AuthProvider/);
  } finally {
    consoleError.mockRestore();
  }
});

test('home links to settings', async () => {
  fetchMock.mockResolvedValueOnce(jsonResponse(200, authResponse()));
  renderApp('/');

  expect(await screen.findByRole('link', { name: 'Cài đặt' })).toHaveAttribute(
    'href',
    '/settings',
  );
  await waitFor(() => {
    expect(currentLocation()).toBe('/');
  });
});
