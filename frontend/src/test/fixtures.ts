import type { AuthResponse } from '../shared/api/types';

export function authResponse(
  email = 'an@example.com',
  token = 'access-token',
): AuthResponse {
  return {
    access_token: token,
    token_type: 'Bearer',
    expires_in: 900,
    user: {
      id: '01936f3a-1b2c-7d4e-8f00-112233445566',
      email,
      role: 'user',
      gender: null,
    },
  };
}
