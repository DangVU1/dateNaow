import { Outlet } from 'react-router';
import styles from './AppLayout.module.css';

export function AppLayout() {
  return (
    <main className={styles.layout}>
      <Outlet />
    </main>
  );
}
