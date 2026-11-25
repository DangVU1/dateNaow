import { useMutation } from '@tanstack/react-query';
import { useEffect, useState, type FormEvent } from 'react';
import { Link } from 'react-router';
import { ApiError, type FieldErrors } from '../../../shared/api/errors';
import { Button } from '../../../shared/ui/Button';
import { FormMessage } from '../../../shared/ui/FormMessage';
import { TextField } from '../../../shared/ui/TextField';
import { confirmPasswordReset } from '../api';
import { anyErrorMessage, fieldMessage } from '../messages';
import { useAuth } from '../useAuth';
import { validateNewPassword } from '../validation';
import styles from './AuthPage.module.css';

const EXPIRED_MESSAGE =
  'Link này đã hết hạn hoặc đã được dùng. Bạn xin link mới nhé.';

interface ResetInput {
  token: string;
  newPassword: string;
}

/**
 * The emailed link is `/reset-password#token=...` (P6). The fragment never
 * reaches a server, so it is read from the address bar itself.
 */
function readTokenFromAddressBar(): string | null {
  const token = new URLSearchParams(window.location.hash.slice(1)).get('token');
  return token === null || token === '' ? null : token;
}

function hasErrors(fields: FieldErrors): boolean {
  return Object.keys(fields).length > 0;
}

export function ResetPasswordPage() {
  const { status, signOut } = useAuth();
  // Read once: the effect below strips the hash, and later renders (or the
  // StrictMode re-run) must still have the token. It is never rendered.
  const [token] = useState(readTokenFromAddressBar);
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [clientErrors, setClientErrors] = useState<FieldErrors>({});
  // Kept outside the mutation: signing out clears the query client.
  const [done, setDone] = useState(false);
  const mutation = useMutation({
    mutationFn: ({ token, newPassword }: ResetInput) =>
      confirmPasswordReset(token, newPassword),
    onSuccess: () => {
      setDone(true);
    },
  });

  // Take the token out of the address bar and history. History state is
  // passed through so React Router keeps its own entry data.
  useEffect(() => {
    if (window.location.hash === '') return;
    const { pathname, search } = window.location;
    window.history.replaceState(window.history.state, '', pathname + search);
  }, []);

  // The reset revoked every refresh token on the server; end the local
  // session too, so this tab does not act as the user until a 401.
  useEffect(() => {
    if (done && status !== 'anonymous') void signOut();
  }, [done, status, signOut]);

  const error = mutation.error;
  const expired =
    token === null ||
    (error instanceof ApiError && error.code === 'invalid_or_expired_token');
  const serverFields: FieldErrors =
    error instanceof ApiError && error.status === 422
      ? (error.fields ?? {})
      : {};
  const fields = hasErrors(clientErrors) ? clientErrors : serverFields;
  const shownOnFields = (serverFields.new_password?.length ?? 0) > 0;
  const formError =
    error !== null && !shownOnFields ? anyErrorMessage(error) : null;

  const messageFor = (field: string): string | undefined => {
    const code = fields[field]?.[0];
    return code === undefined ? undefined : fieldMessage(field, code);
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (mutation.isPending || token === null) return;
    const errors = validateNewPassword(newPassword, confirmPassword);
    setClientErrors(errors);
    if (hasErrors(errors)) {
      mutation.reset();
      return;
    }
    mutation.mutate({ token, newPassword });
  };

  let content;
  if (done) {
    content = (
      <>
        <p role="status" className={styles.notice}>
          Đổi mật khẩu xong. Mời bạn đăng nhập lại.
        </p>
        <div className={styles.links}>
          <p>
            <Link to="/sign-in">Đăng nhập</Link>
          </p>
        </div>
      </>
    );
  } else if (expired) {
    content = (
      <>
        <FormMessage message={EXPIRED_MESSAGE} />
        <div className={styles.links}>
          <p>
            <Link to="/forgot-password">Xin link mới</Link>
          </p>
        </div>
      </>
    );
  } else {
    content = (
      <form className={styles.form} onSubmit={handleSubmit} noValidate>
        <TextField
          label="Mật khẩu mới"
          name="new_password"
          type="password"
          autoComplete="new-password"
          value={newPassword}
          onChange={(e) => {
            setNewPassword(e.target.value);
          }}
          error={messageFor('new_password')}
        />
        <TextField
          label="Nhập lại mật khẩu"
          name="confirm_password"
          type="password"
          autoComplete="new-password"
          value={confirmPassword}
          onChange={(e) => {
            setConfirmPassword(e.target.value);
          }}
          error={messageFor('confirm_password')}
        />
        <FormMessage message={formError} />
        <Button type="submit" disabled={mutation.isPending}>
          Đổi mật khẩu
        </Button>
      </form>
    );
  }

  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Đặt lại mật khẩu</h1>
      {content}
    </section>
  );
}
