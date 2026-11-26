import { useMutation } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { Link } from 'react-router';
import { ApiError, type FieldErrors } from '../../../shared/api/errors';
import { Button } from '../../../shared/ui/Button';
import { FormMessage } from '../../../shared/ui/FormMessage';
import { TextField } from '../../../shared/ui/TextField';
import { requestPasswordReset } from '../api';
import { anyErrorMessage, fieldMessage } from '../messages';
import { validateEmail } from '../validation';
import styles from './AuthPage.module.css';

const CONFIRMATION =
  'Nếu email này có tài khoản, bạn sẽ nhận được link đặt lại mật khẩu trong vài phút.';

/**
 * Asks for a reset link. The backend answers 204 whether or not the email has
 * an account (P6), so success always shows the same sentence. Errors that do
 * not depend on the account (429, a malformed email, no connection, a server
 * fault) are shown so the user knows nothing was sent and can retry.
 */
export function ForgotPasswordPage() {
  const [email, setEmail] = useState('');
  const [clientErrors, setClientErrors] = useState<FieldErrors>({});
  const mutation = useMutation({
    mutationFn: (email: string) => requestPasswordReset(email),
  });

  const error = mutation.error;
  const serverEmailCode =
    error instanceof ApiError && error.status === 422
      ? error.fields?.email?.[0]
      : undefined;
  const emailCode = clientErrors.email?.[0] ?? serverEmailCode;
  const formError =
    error !== null && serverEmailCode === undefined
      ? anyErrorMessage(error)
      : null;

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (mutation.isPending) return;
    const errors = validateEmail(email);
    setClientErrors(errors);
    if (Object.keys(errors).length > 0) {
      mutation.reset();
      return;
    }
    mutation.mutate(email);
  };

  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Quên mật khẩu</h1>
      {mutation.isSuccess ? (
        <p role="status" className={styles.notice}>
          {CONFIRMATION}
        </p>
      ) : (
        <form className={styles.form} onSubmit={handleSubmit} noValidate>
          <p className={styles.lead}>
            Nhập email của tài khoản. Bạn sẽ nhận được link để đặt mật khẩu mới.
          </p>
          <TextField
            label="Email"
            name="email"
            type="email"
            inputMode="email"
            autoComplete="email"
            autoCapitalize="none"
            spellCheck={false}
            value={email}
            onChange={(e) => {
              setEmail(e.target.value);
            }}
            error={
              emailCode === undefined
                ? undefined
                : fieldMessage('email', emailCode)
            }
          />
          <FormMessage message={formError} />
          <Button type="submit" disabled={mutation.isPending}>
            Gửi link đặt lại
          </Button>
        </form>
      )}
      <div className={styles.links}>
        <p>
          <Link to="/sign-in">Về trang đăng nhập</Link>
        </p>
      </div>
    </section>
  );
}
