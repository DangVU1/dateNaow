import { createContext, useContext } from 'react';
import type { UserDto } from '../../shared/api/types';

export type AuthStatus = 'loading' | 'authenticated' | 'anonymous';

export interface AuthContextValue {
  status: AuthStatus;
  user: UserDto | null;
  signIn: (email: string, password: string) => Promise<void>;
  signUp: (email: string, password: string) => Promise<void>;
  /** Never rejects: the local session is cleared even if the API call fails. */
  signOut: () => Promise<void>;
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
