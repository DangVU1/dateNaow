import { apiFetch } from '../../shared/api/client';
import type { AuthResponse } from '../../shared/api/types';

// None of these send the access token: they work from credentials or the
// refresh cookie, and a 401 here must not trigger a token refresh.

export function register(
  email: string,
  password: string,
): Promise<AuthResponse> {
  return apiFetch<AuthResponse>('/auth/register', {
    method: 'POST',
    json: { email, password },
    auth: false,
  });
}

export function login(email: string, password: string): Promise<AuthResponse> {
  return apiFetch<AuthResponse>('/auth/login', {
    method: 'POST',
    json: { email, password },
    auth: false,
  });
}

export function logout(): Promise<void> {
  return apiFetch<void>('/auth/logout', { method: 'POST', auth: false });
}

export function requestPasswordReset(email: string): Promise<void> {
  return apiFetch<void>('/auth/password-reset/request', {
    method: 'POST',
    json: { email },
    auth: false,
  });
}

export function confirmPasswordReset(
  token: string,
  newPassword: string,
): Promise<void> {
  return apiFetch<void>('/auth/password-reset/confirm', {
    method: 'POST',
    json: { token, new_password: newPassword },
    auth: false,
  });
}
