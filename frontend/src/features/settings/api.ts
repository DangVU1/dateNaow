import { apiFetch } from '../../shared/api/client';
import type { Gender, UserDto } from '../../shared/api/types';

/** `PATCH /me`. `null` clears the choice; the field is always sent. */
export function updateGender(gender: Gender | null): Promise<UserDto> {
  return apiFetch<UserDto>('/me', { method: 'PATCH', json: { gender } });
}
