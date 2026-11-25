import { Link, useLocation } from 'react-router';
import { useAuth } from '../useAuth';
import styles from './AuthPage.module.css';
import { CredentialsForm } from './CredentialsForm';

export function SignUpPage() {
  const { signUp } = useAuth();
  const { state: locationState } = useLocation() as { state: unknown };

  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Đăng ký</h1>
      {/* Sign-up also signs in; the GuestOnly guard then leaves this page. */}
      <CredentialsForm
        submitLabel="Tạo tài khoản"
        passwordAutoComplete="new-password"
        onSubmit={signUp}
      />
      <div className={styles.links}>
        <p>
          Đã có tài khoản?{' '}
          <Link to="/sign-in" state={locationState}>
            Đăng nhập
          </Link>
        </p>
      </div>
    </section>
  );
}
