import { Link, Outlet } from 'react-router';
import { useAuth } from '../../features/auth/useAuth';

/**
 * Route wrapper inside `RequireAuth`: only admins see the child routes. This
 * is a courtesy for the UI; the admin API checks the role itself (P7).
 */
export function RequireAdmin() {
  const { user } = useAuth();

  if (user?.role !== 'admin') {
    return (
      <section>
        <h1>Bạn không có quyền vào trang này.</h1>
        <p>
          <Link to="/">Về trang chủ</Link>
        </p>
      </section>
    );
  }
  return <Outlet />;
}
