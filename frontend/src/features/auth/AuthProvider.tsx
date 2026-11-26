import { useQueryClient } from '@tanstack/react-query';
import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  type ReactNode,
} from 'react';
import {
  onSessionExpired,
  refreshAccessToken,
  waitForPendingRefresh,
} from '../../shared/api/client';
import { ApiError } from '../../shared/api/errors';
import { setAccessToken } from '../../shared/api/tokenStore';
import type { AuthResponse, UserDto } from '../../shared/api/types';
import { login, logout, register } from './api';
import {
  AuthContext,
  type AuthContextValue,
  type AuthStatus,
  type SignOutResult,
} from './useAuth';

interface AuthState {
  status: AuthStatus;
  user: UserDto | null;
  /** The last sign-out was not confirmed by the server (DECISIONS #17). */
  signOutFailed: boolean;
}

type AuthAction =
  | { type: 'authenticated'; user: UserDto }
  | { type: 'user_updated'; user: UserDto }
  | { type: 'anonymous' }
  | { type: 'signed_out'; confirmed: boolean };

/** The backend's own 500 envelope, which still clears the refresh cookie. */
function isBackendInternalError(err: unknown): boolean {
  return (
    err instanceof ApiError &&
    err.status === 500 &&
    err.code === 'internal_error'
  );
}

const initialState: AuthState = {
  status: 'loading',
  user: null,
  signOutFailed: false,
};

function reducer(state: AuthState, action: AuthAction): AuthState {
  switch (action.type) {
    case 'authenticated':
      return {
        status: 'authenticated',
        user: action.user,
        signOutFailed: false,
      };
    case 'user_updated':
      return state.status === 'authenticated'
        ? { ...state, user: action.user }
        : state;
    case 'anonymous':
      return state.status === 'anonymous'
        ? state
        : { status: 'anonymous', user: null, signOutFailed: false };
    case 'signed_out':
      return {
        status: 'anonymous',
        user: null,
        signOutFailed: !action.confirmed,
      };
  }
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const [state, dispatch] = useReducer(reducer, initialState);
  // Bumped whenever a session starts or ends here. A slow async result
  // (the restore refresh) that started under an older generation is stale
  // and must not overwrite what the user did meanwhile.
  const generation = useRef(0);

  // Restore the session from the refresh cookie. Under StrictMode this
  // effect runs twice; `refreshAccessToken` is single-flight, so both runs
  // share one request, and only the still-mounted run applies the result.
  useEffect(() => {
    let active = true;
    const startedAt = generation.current;
    const apply = (result: AuthResponse | null) => {
      if (!active || generation.current !== startedAt) return;
      dispatch(
        result === null
          ? { type: 'anonymous' }
          : { type: 'authenticated', user: result.user },
      );
    };
    void refreshAccessToken().then(apply, () => {
      apply(null);
    });
    return () => {
      active = false;
    };
  }, []);

  // `apiFetch` already cleared the token; drop cached data of the old user.
  useEffect(
    () =>
      onSessionExpired(() => {
        generation.current += 1;
        queryClient.clear();
        dispatch({ type: 'anonymous' });
      }),
    [queryClient],
  );

  const startSession = useCallback((response: AuthResponse) => {
    generation.current += 1;
    setAccessToken(response.access_token);
    dispatch({ type: 'authenticated', user: response.user });
  }, []);

  // A refresh still in flight (e.g. the restore on mount) would store the
  // previous user's token and rotated cookie after the new session starts,
  // so let it settle before signing in or up.
  const signIn = useCallback(
    async (email: string, password: string) => {
      await waitForPendingRefresh();
      startSession(await login(email, password));
    },
    [startSession],
  );

  const signUp = useCallback(
    async (email: string, password: string) => {
      await waitForPendingRefresh();
      startSession(await register(email, password));
    },
    [startSession],
  );

  const signOut = useCallback(async () => {
    generation.current += 1;
    // A refresh already in flight would rotate the cookie and store a new
    // token. Let it settle first, so logout revokes the newest cookie and
    // nothing re-sets the token after it is cleared below.
    await waitForPendingRefresh();
    let result: SignOutResult = 'signed_out';
    try {
      await logout();
    } catch (err) {
      // The server answers 204 even for unknown tokens, so a failure is one
      // of two cases. The backend's own 500 (#17): the refresh token could
      // not be revoked, but that response cleared the cookie, so sign out
      // locally and let the sign-in page say the server did not confirm.
      // Anything else (no response, a proxy error): the cookie may still be
      // there, so signing out locally would be untrue; keep the session and
      // let the caller offer a retry.
      result = isBackendInternalError(err) ? 'not_confirmed' : 'not_reached';
    }
    if (result === 'not_reached') return result;
    await waitForPendingRefresh();
    setAccessToken(null);
    queryClient.clear();
    dispatch({ type: 'signed_out', confirmed: result === 'signed_out' });
    return result;
  }, [queryClient]);

  const setUser = useCallback((user: UserDto) => {
    dispatch({ type: 'user_updated', user });
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      status: state.status,
      user: state.user,
      signOutFailed: state.signOutFailed,
      signIn,
      signUp,
      signOut,
      setUser,
    }),
    [state, signIn, signUp, signOut, setUser],
  );

  return <AuthContext value={value}>{children}</AuthContext>;
}
