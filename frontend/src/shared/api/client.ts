import { ApiError, networkError, toApiError } from './errors';
import { getAccessToken, setAccessToken } from './tokenStore';
import type { AuthResponse } from './types';

const API_PREFIX = '/api/v1';
const REFRESH_CONFLICT_DELAY_MS = 500;

export interface ApiFetchOptions {
  method?: string;
  /** Sent as a JSON body with `Content-Type: application/json`. */
  json?: unknown;
  /** Attach the access token and refresh it on 401. Defaults to true. */
  auth?: boolean;
}

/**
 * Calls `/api/v1{path}` and returns the parsed JSON body (`undefined` on 204).
 * Non-2xx responses and network failures reject with `ApiError`.
 *
 * With `auth` on, a 401 triggers one shared refresh and exactly one retry.
 * If the refresh fails, or the retry is rejected with 401 again, the session
 * is expired: the token is cleared and `onSessionExpired` listeners run.
 */
export async function apiFetch<T>(
  path: string,
  options: ApiFetchOptions = {},
): Promise<T> {
  const auth = options.auth ?? true;
  const sentToken = auth ? getAccessToken() : null;
  const sentEpoch = sessionEpoch;
  let res = await send(path, options, sentToken);

  if (res.status === 401 && auth) {
    let token = getAccessToken();
    // Another call may already have refreshed while this one was in flight;
    // then reuse its token instead of rotating the cookie again.
    if (token === null || token === sentToken) {
      // Another call already ended this session: do not refresh again.
      if (sessionEpoch !== sentEpoch) throw await toApiError(res);
      const refreshed = await refreshAccessToken();
      if (refreshed === null) {
        expireSession(sentEpoch);
        throw await toApiError(res);
      }
      token = refreshed.access_token;
    }
    res = await send(path, options, token);
    if (res.status === 401) {
      // A token the server just issued was rejected. Refreshing again could
      // loop, so the session is treated as over.
      expireSession(sentEpoch);
      throw await toApiError(res);
    }
  }

  if (!res.ok) throw await toApiError(res);
  if (res.status === 204) return undefined as T;
  try {
    return (await res.json()) as T;
  } catch (error) {
    throw new ApiError(res.status, 'unknown_error', undefined, {
      cause: error,
    });
  }
}

async function send(
  path: string,
  options: ApiFetchOptions,
  token: string | null,
): Promise<Response> {
  const headers: Record<string, string> = {};
  const init: RequestInit = {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers,
  };
  if (options.json !== undefined) {
    headers['Content-Type'] = 'application/json';
    init.body = JSON.stringify(options.json);
  }
  if (token !== null) headers.Authorization = `Bearer ${token}`;
  try {
    return await fetch(`${API_PREFIX}${path}`, init);
  } catch (error) {
    throw networkError(error);
  }
}

let refreshInFlight: Promise<AuthResponse | null> | null = null;

/**
 * Rotates the refresh cookie and stores the new access token. Concurrent
 * callers share one request. Resolves `null` on any failure, including a
 * network error. It does not clear the stored token or notify listeners
 * itself: `apiFetch` does that, and a direct caller (e.g. at app start)
 * decides on its own.
 */
export function refreshAccessToken(): Promise<AuthResponse | null> {
  refreshInFlight ??= refreshWithConflictRetry().finally(() => {
    refreshInFlight = null;
  });
  return refreshInFlight;
}

/**
 * Resolves once no refresh is running. Sign-out awaits this so a refresh
 * that was already in flight cannot store a token after the session ended,
 * and so the logout request carries the newest refresh cookie.
 */
export async function waitForPendingRefresh(): Promise<void> {
  while (refreshInFlight !== null) {
    await refreshInFlight;
  }
}

async function refreshWithConflictRetry(): Promise<AuthResponse | null> {
  try {
    return await refreshOnce();
  } catch (error) {
    // 409 `refresh_conflict`: another tab rotated the cookie moments ago and
    // the browser now holds the new one, so one delayed retry succeeds.
    const isConflict =
      error instanceof ApiError &&
      error.status === 409 &&
      error.code === 'refresh_conflict';
    if (!isConflict) return null;
  }
  await delay(REFRESH_CONFLICT_DELAY_MS);
  try {
    return await refreshOnce();
  } catch {
    return null;
  }
}

async function refreshOnce(): Promise<AuthResponse> {
  const body = await apiFetch<AuthResponse>('/auth/refresh', {
    method: 'POST',
    auth: false,
  });
  setAccessToken(body.access_token);
  return body;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

const sessionExpiredListeners = new Set<() => void>();

/** Runs `listener` whenever the session ends; returns the unsubscribe. */
export function onSessionExpired(listener: () => void): () => void {
  sessionExpiredListeners.add(listener);
  return () => {
    sessionExpiredListeners.delete(listener);
  };
}

/**
 * Bumped each time the session expires. A call that started in an earlier
 * epoch neither refreshes nor notifies again, so concurrent or late 401s
 * end the session exactly once.
 */
let sessionEpoch = 0;

function expireSession(epochAtSend: number): void {
  if (epochAtSend !== sessionEpoch) return;
  sessionEpoch += 1;
  setAccessToken(null);
  for (const listener of [...sessionExpiredListeners]) {
    try {
      listener();
    } catch (error) {
      // One faulty listener must not skip the others or replace the ApiError.
      console.error('onSessionExpired listener failed', error);
    }
  }
}
