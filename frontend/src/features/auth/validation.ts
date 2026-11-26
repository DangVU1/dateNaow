import type { FieldErrors } from '../../shared/api/errors';

// Mirrors the backend rules (P4) so obvious mistakes never reach the API.
export const MIN_PASSWORD_CHARS = 8;
export const MAX_PASSWORD_CHARS = 128;

/** Characters as the backend counts them: Unicode code points, not UTF-16 units. */
function charCount(value: string): number {
  return [...value].length;
}

/** Returns the error code for `password`, or null when it is acceptable. */
export function passwordError(password: string): string | null {
  const chars = charCount(password);
  if (chars < MIN_PASSWORD_CHARS) return 'too_short';
  if (chars > MAX_PASSWORD_CHARS) return 'too_long';
  return null;
}

/** Client-side checks before sign-in or sign-up; same shape as a 422. */
export function validateCredentials(
  email: string,
  password: string,
): FieldErrors {
  const errors = validateEmail(email);
  const pwError = passwordError(password);
  if (pwError !== null) errors.password = [pwError];
  return errors;
}

/** Client-side checks before setting a new password from a reset link. */
export function validateNewPassword(
  newPassword: string,
  confirmPassword: string,
): FieldErrors {
  const errors: FieldErrors = {};
  const pwError = passwordError(newPassword);
  if (pwError !== null) errors.new_password = [pwError];
  if (confirmPassword !== newPassword) errors.confirm_password = ['mismatch'];
  return errors;
}

/** Client-side check before asking for a reset link. */
export function validateEmail(email: string): FieldErrors {
  return email.trim() === '' ? { email: ['required'] } : {};
}
