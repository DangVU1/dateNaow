import { useState } from 'react';
import { Link } from 'react-router';
import { useAuth } from '../auth/useAuth';
import { Button } from '../../shared/ui/Button';
import styles from './HomePage.module.css';

// Placeholder home; later plans replace its content.
export function HomePage() {
  const { user, signOut } = useAuth();
  const [signingOut, setSigningOut] = useState(false);

  const handleSignOut = () => {
    setSigningOut(true);
    // signOut never rejects; the guard leaves this page once it finishes.
    void signOut();
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
          onClick={handleSignOut}
          disabled={signingOut}
        >
          Đăng xuất
        </Button>
      </nav>
    </section>
  );
}
