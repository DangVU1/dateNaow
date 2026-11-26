import { Link, useLocation } from 'react-router';
import { FormMessage } from '../../../shared/ui/FormMessage';
import { SIGN_OUT_NOT_CONFIRMED_MESSAGE } from '../messages';
import { useAuth } from '../useAuth';
import styles from './AuthPage.module.css';
import { CredentialsForm } from './CredentialsForm';

export function SignInPage() {
  const { signIn, signOutFailed } = useAuth();
  // Passed on to sign-up so `state.from` survives switching pages.
  const { state: locationState } = useLocation() as { state: unknown };

  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Đăng nhập</h1>
      <FormMessage
        message={signOutFailed ? SIGN_OUT_NOT_CONFIRMED_MESSAGE : null}
      />
      {/* After success the GuestOnly guard sends the user to `state.from`. */}
      <CredentialsForm
        submitLabel="Đăng nhập"
        passwordAutoComplete="current-password"
        onSubmit={signIn}
      />
      <div className={styles.links}>
        <p>
          <Link to="/forgot-password">Quên mật khẩu?</Link>
        </p>
        <p>
          Chưa có tài khoản?{' '}
          <Link to="/sign-up" state={locationState}>
            Đăng ký
          </Link>
        </p>
      </div>
    </section>
  );
}
