import { useState } from 'react';
import { Link } from 'react-router';
import { SIGN_OUT_NOT_REACHED_MESSAGE } from '../auth/messages';
import { useAuth } from '../auth/useAuth';
import { Button } from '../../shared/ui/Button';
import { FormMessage } from '../../shared/ui/FormMessage';
import styles from './HomePage.module.css';

// Placeholder home; later plans replace its content.
export function HomePage() {
  const { user, signOut } = useAuth();
  const [signingOut, setSigningOut] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSignOut = async () => {
    setSigningOut(true);
    setError(null);
    // signOut never rejects. Once it signs out, the guard leaves this page
    // (and the sign-in page says if the server did not confirm it). If the
    // request got no backend answer, nothing changed: offer a retry.
    if ((await signOut()) === 'not_reached') {
      setError(SIGN_OUT_NOT_REACHED_MESSAGE);
      setSigningOut(false);
    }
  };

  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Xin chào, {user?.email}</h1>
      <nav className={styles.actions}>
        <Link to="/settings" className={styles.link}>
          Cài đặt
        </Link>
        <Button
          variant="secondary"
          onClick={() => {
            void handleSignOut();
          }}
          disabled={signingOut}
        >
          Đăng xuất
        </Button>
        <FormMessage message={error} />
      </nav>
    </section>
  );
}
