import { createContext, useContext } from 'react';
import type { UserDto } from '../../shared/api/types';

export type AuthStatus = 'loading' | 'authenticated' | 'anonymous';

export type SignOutResult = 'signed_out' | 'not_confirmed' | 'not_reached';

export interface AuthContextValue {
  status: AuthStatus;
  user: UserDto | null;
  /**
   * The last `signOut` cleared the local session but the backend answered
   * 500, so that session may still be live on the server. Reset when a new
   * session starts.
   */
  signOutFailed: boolean;
  signIn: (email: string, password: string) => Promise<void>;
  signUp: (email: string, password: string) => Promise<void>;
  /**
   * Never rejects. `signed_out`: done. `not_confirmed`: signed out locally,
   * but the server could not revoke the session. `not_reached`: the request
   * did not get a backend answer, so nothing changed and the caller should
   * offer a retry.
   */
  signOut: () => Promise<SignOutResult>;
  setUser: (user: UserDto) => void;
}

// Kept apart from `AuthProvider.tsx` so that file exports only a component
// (react-refresh fast refresh).
export const AuthContext = createContext<AuthContextValue | null>(null);

export function useAuth(): AuthContextValue {
  const value = useContext(AuthContext);
  if (value === null) {
    throw new Error('useAuth must be used inside <AuthProvider>');
  }
  return value;
}
