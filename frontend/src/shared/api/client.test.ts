import { apiFetch, onSessionExpired, refreshAccessToken } from './client';
import { ApiError } from './errors';
import { getAccessToken, setAccessToken } from './tokenStore';
import type { AuthResponse } from './types';

const fetchMock = vi.fn<typeof fetch>();

function jsonResponse(
  status: number,
  body: unknown,
  headers: Record<string, string> = {},
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', ...headers },
  });
}

function errorResponse(
  status: number,
  code: string,
  headers: Record<string, string> = {},
): Response {
  return jsonResponse(status, { error: { code, message: 'msg' } }, headers);
}

function authResponse(token: string): AuthResponse {
  return {
    access_token: token,
    token_type: 'Bearer',
    expires_in: 900,
    user: {
      id: '01936f3a-1b2c-7d4e-8f00-112233445566',
      email: 'an@example.com',
      role: 'user',
      gender: null,
    },
  };
}

function urlOf(input: RequestInfo | URL): string {
  if (typeof input === 'string') return input;
  if (input instanceof URL) return input.href;
  return input.url;
}

function calledUrls(): string[] {
  return fetchMock.mock.calls.map(([input]) => urlOf(input));
}

function headersOf(callIndex: number): Headers {
  const init = fetchMock.mock.calls[callIndex]?.[1];
  return new Headers(init?.headers);
}

beforeEach(() => {
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  setAccessToken(null);
});

test('adds bearer header when a token is stored', async () => {
  setAccessToken('tok-1');
  fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: true }));

  const result = await apiFetch<{ ok: boolean }>('/me');

  expect(result).toEqual({ ok: true });
  expect(calledUrls()).toEqual(['/api/v1/me']);
  const init = fetchMock.mock.calls[0]?.[1];
  expect(init?.credentials).toBe('same-origin');
  expect(init?.method).toBe('GET');
  expect(headersOf(0).get('Authorization')).toBe('Bearer tok-1');
  expect(headersOf(0).has('Content-Type')).toBe(false);
});

test('sends json body with content type only when json is given', async () => {
  fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: true }));

  await apiFetch('/me', { method: 'PATCH', json: { gender: 'female' } });

  const init = fetchMock.mock.calls[0]?.[1];
  expect(init?.method).toBe('PATCH');
  expect(init?.body).toBe('{"gender":"female"}');
  expect(headersOf(0).get('Content-Type')).toBe('application/json');
  expect(headersOf(0).has('Authorization')).toBe(false);
});

test('on 401 refreshes once and retries the original request', async () => {
  setAccessToken('old');
  fetchMock
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
    .mockResolvedValueOnce(jsonResponse(200, authResponse('new')))
    .mockResolvedValueOnce(jsonResponse(200, { id: 'me' }));

  const result = await apiFetch<{ id: string }>('/me');

  expect(result).toEqual({ id: 'me' });
  expect(calledUrls()).toEqual([
    '/api/v1/me',
    '/api/v1/auth/refresh',
    '/api/v1/me',
  ]);
  expect(fetchMock.mock.calls[1]?.[1]?.method).toBe('POST');
  expect(headersOf(1).has('Authorization')).toBe(false);
  expect(headersOf(2).get('Authorization')).toBe('Bearer new');
  expect(getAccessToken()).toBe('new');
});

test('concurrent 401s trigger a single refresh request', async () => {
  setAccessToken('old');
  let releaseRefresh: (res: Response) => void = () => undefined;
  const refreshGate = new Promise<Response>((resolve) => {
    releaseRefresh = resolve;
  });
  fetchMock.mockImplementation((input, init) => {
    const url = urlOf(input);
    if (url === '/api/v1/auth/refresh') return refreshGate;
    const auth = new Headers(init?.headers).get('Authorization');
    return Promise.resolve(
      auth === 'Bearer new'
        ? jsonResponse(200, { url })
        : errorResponse(401, 'unauthorized'),
    );
  });

  const calls = Promise.all([
    apiFetch<{ url: string }>('/a'),
    apiFetch<{ url: string }>('/b'),
    apiFetch<{ url: string }>('/c'),
  ]);
  await vi.waitFor(() => {
    expect(calledUrls().filter((u) => u === '/api/v1/auth/refresh')).toEqual([
      '/api/v1/auth/refresh',
    ]);
    expect(calledUrls()).toHaveLength(4);
  });
  releaseRefresh(jsonResponse(200, authResponse('new')));

  await expect(calls).resolves.toEqual([
    { url: '/api/v1/a' },
    { url: '/api/v1/b' },
    { url: '/api/v1/c' },
  ]);
  expect(calledUrls().filter((u) => u === '/api/v1/auth/refresh')).toHaveLength(
    1,
  );
  expect(calledUrls()).toHaveLength(7);
});

test('when refresh fails it clears the token, notifies listeners and throws 401', async () => {
  setAccessToken('old');
  const listener = vi.fn();
  const unsubscribe = onSessionExpired(listener);
  fetchMock
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
    .mockResolvedValueOnce(errorResponse(401, 'invalid_refresh_token'));

  const error = await apiFetch('/me').catch((e: unknown) => e);

  expect(error).toBeInstanceOf(ApiError);
  expect(error).toMatchObject({ status: 401 });
  expect(getAccessToken()).toBeNull();
  expect(listener).toHaveBeenCalledTimes(1);
  expect(calledUrls()).toEqual(['/api/v1/me', '/api/v1/auth/refresh']);

  unsubscribe();
  fetchMock
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
    .mockResolvedValueOnce(errorResponse(401, 'invalid_refresh_token'));
  await expect(apiFetch('/me')).rejects.toBeInstanceOf(ApiError);
  expect(listener).toHaveBeenCalledTimes(1);
});

test('a 401 on the retried request does not refresh again', async () => {
  setAccessToken('old');
  const listener = vi.fn();
  const unsubscribe = onSessionExpired(listener);
  fetchMock
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
    .mockResolvedValueOnce(jsonResponse(200, authResponse('new')))
    .mockResolvedValueOnce(errorResponse(401, 'unauthorized'));

  await expect(apiFetch('/me')).rejects.toMatchObject({
    status: 401,
    code: 'unauthorized',
  });

  expect(calledUrls()).toEqual([
    '/api/v1/me',
    '/api/v1/auth/refresh',
    '/api/v1/me',
  ]);
  expect(getAccessToken()).toBeNull();
  expect(listener).toHaveBeenCalledTimes(1);
  unsubscribe();
});

test('refresh_conflict is retried once after 500ms', async () => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  fetchMock
    .mockResolvedValueOnce(errorResponse(409, 'refresh_conflict'))
    .mockResolvedValueOnce(jsonResponse(200, authResponse('new')));

  const pending = refreshAccessToken();
  await vi.advanceTimersByTimeAsync(499);
  expect(fetchMock).toHaveBeenCalledTimes(1);

  await vi.advanceTimersByTimeAsync(1);
  expect(fetchMock).toHaveBeenCalledTimes(2);

  await expect(pending).resolves.toEqual(authResponse('new'));
  expect(getAccessToken()).toBe('new');
  expect(calledUrls()).toEqual([
    '/api/v1/auth/refresh',
    '/api/v1/auth/refresh',
  ]);
});

test('a second refresh_conflict gives up with null', async () => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  fetchMock
    .mockResolvedValueOnce(errorResponse(409, 'refresh_conflict'))
    .mockResolvedValueOnce(errorResponse(409, 'refresh_conflict'));

  const pending = refreshAccessToken();
  await vi.advanceTimersByTimeAsync(2000);

  await expect(pending).resolves.toBeNull();
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

test('refresh resolves null on network error', async () => {
  fetchMock.mockRejectedValueOnce(new TypeError('Failed to fetch'));

  await expect(refreshAccessToken()).resolves.toBeNull();
});

test('parses error body into ApiError with code and fields', async () => {
  fetchMock.mockResolvedValueOnce(
    jsonResponse(422, {
      error: {
        code: 'validation_error',
        message: 'Một số thông tin chưa hợp lệ.',
        fields: { password: ['too_short'] },
      },
    }),
  );

  const error = await apiFetch('/auth/register', {
    method: 'POST',
    json: {},
    auth: false,
  }).catch((e: unknown) => e);

  expect(error).toBeInstanceOf(ApiError);
  expect(error).toMatchObject({
    name: 'ApiError',
    status: 422,
    code: 'validation_error',
    message: 'Một số thông tin chưa hợp lệ.',
    fields: { password: ['too_short'] },
  });
});

test('non-JSON error body gives unknown_error', async () => {
  fetchMock.mockResolvedValueOnce(
    new Response('<html>Bad gateway</html>', { status: 502 }),
  );

  await expect(apiFetch('/me', { auth: false })).rejects.toMatchObject({
    status: 502,
    code: 'unknown_error',
  });
});

test('reads Retry-After into retryAfterSecs', async () => {
  fetchMock.mockResolvedValueOnce(
    errorResponse(429, 'rate_limited', { 'Retry-After': '120' }),
  );

  await expect(
    apiFetch('/auth/login', { method: 'POST', json: {}, auth: false }),
  ).rejects.toMatchObject({
    status: 429,
    code: 'rate_limited',
    retryAfterSecs: 120,
  });
});

test('wraps a thrown fetch into a network_error ApiError', async () => {
  fetchMock.mockRejectedValueOnce(new TypeError('Failed to fetch'));

  await expect(apiFetch('/me')).rejects.toMatchObject({
    name: 'ApiError',
    status: 0,
    code: 'network_error',
  });
});

test('does not try to refresh when auth is false', async () => {
  setAccessToken('tok-1');
  fetchMock.mockResolvedValueOnce(errorResponse(401, 'invalid_credentials'));

  await expect(
    apiFetch('/auth/login', { method: 'POST', json: {}, auth: false }),
  ).rejects.toMatchObject({ status: 401, code: 'invalid_credentials' });

  expect(calledUrls()).toEqual(['/api/v1/auth/login']);
  expect(headersOf(0).has('Authorization')).toBe(false);
  expect(getAccessToken()).toBe('tok-1');
});

test('resolves undefined on 204', async () => {
  fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));

  await expect(
    apiFetch('/auth/logout', { method: 'POST' }),
  ).resolves.toBeUndefined();
});

test('concurrent 401s with a failing refresh expire the session exactly once', async () => {
  setAccessToken('old');
  const listener = vi.fn();
  const unsubscribe = onSessionExpired(listener);
  try {
    fetchMock.mockImplementation((input) =>
      Promise.resolve(
        urlOf(input).endsWith('/auth/refresh')
          ? errorResponse(401, 'invalid_refresh_token')
          : errorResponse(401, 'unauthorized'),
      ),
    );

    const results = await Promise.allSettled([
      apiFetch('/a'),
      apiFetch('/b'),
      apiFetch('/c'),
    ]);

    expect(results.map((r) => r.status)).toEqual([
      'rejected',
      'rejected',
      'rejected',
    ]);
    expect(
      calledUrls().filter((url) => url.endsWith('/auth/refresh')),
    ).toHaveLength(1);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(getAccessToken()).toBeNull();
  } finally {
    unsubscribe();
  }
});

test('a 401 arriving after the session expired does not refresh again', async () => {
  setAccessToken('old');
  const listener = vi.fn();
  const unsubscribe = onSessionExpired(listener);
  try {
    let releaseLate: (res: Response) => void = () => undefined;
    fetchMock
      // late request, sent first with the old token, answered last
      .mockImplementationOnce(
        () =>
          new Promise<Response>((resolve) => {
            releaseLate = resolve;
          }),
      )
      .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
      .mockResolvedValueOnce(errorResponse(401, 'invalid_refresh_token'));

    const late = apiFetch('/late').catch((e: unknown) => e);
    await expect(apiFetch('/first')).rejects.toMatchObject({ status: 401 });
    expect(listener).toHaveBeenCalledTimes(1);

    releaseLate(errorResponse(401, 'unauthorized'));
    expect(await late).toMatchObject({ status: 401 });

    expect(calledUrls()).toEqual([
      '/api/v1/late',
      '/api/v1/first',
      '/api/v1/auth/refresh',
    ]);
    expect(listener).toHaveBeenCalledTimes(1);
  } finally {
    unsubscribe();
  }
});

test('a 401 for a stale token retries with the newer token without refreshing', async () => {
  setAccessToken('old');
  let releaseStale: (res: Response) => void = () => undefined;
  fetchMock
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          releaseStale = resolve;
        }),
    )
    .mockResolvedValueOnce(jsonResponse(200, { ok: true }));

  const pending = apiFetch<{ ok: boolean }>('/me');
  setAccessToken('new'); // another call refreshed meanwhile
  releaseStale(errorResponse(401, 'unauthorized'));

  await expect(pending).resolves.toEqual({ ok: true });
  expect(calledUrls()).toEqual(['/api/v1/me', '/api/v1/me']);
  expect(headersOf(1).get('Authorization')).toBe('Bearer new');
});

test('a network error during refresh expires the session', async () => {
  setAccessToken('old');
  const listener = vi.fn();
  const unsubscribe = onSessionExpired(listener);
  try {
    fetchMock
      .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
      .mockRejectedValueOnce(new TypeError('Failed to fetch'));

    await expect(apiFetch('/me')).rejects.toMatchObject({ status: 401 });
    expect(getAccessToken()).toBeNull();
    expect(listener).toHaveBeenCalledTimes(1);
  } finally {
    unsubscribe();
  }
});

test('a throwing listener does not skip others or replace the ApiError', async () => {
  setAccessToken('old');
  const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
  const bad = vi.fn(() => {
    throw new Error('boom');
  });
  const good = vi.fn();
  const unsubscribeBad = onSessionExpired(bad);
  const unsubscribeGood = onSessionExpired(good);
  try {
    fetchMock
      .mockResolvedValueOnce(errorResponse(401, 'unauthorized'))
      .mockResolvedValueOnce(errorResponse(401, 'invalid_refresh_token'));

    await expect(apiFetch('/me')).rejects.toBeInstanceOf(ApiError);
    expect(bad).toHaveBeenCalledTimes(1);
    expect(good).toHaveBeenCalledTimes(1);
  } finally {
    unsubscribeBad();
    unsubscribeGood();
    consoleError.mockRestore();
  }
});
