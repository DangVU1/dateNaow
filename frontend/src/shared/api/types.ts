// Mirrors the backend response DTOs (`UserResponse`, auth `AuthResponse`).

export type Gender = 'male' | 'female' | 'other' | 'prefer_not_to_say';

export interface UserDto {
  id: string;
  email: string;
  role: 'user' | 'admin';
  gender: Gender | null;
}

export interface AuthResponse {
  access_token: string;
  token_type: 'Bearer';
  /** Access-token lifetime in seconds. */
  expires_in: number;
  user: UserDto;
}
