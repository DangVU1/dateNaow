import { Navigate, Outlet, useLocation } from 'react-router';
import { useAuth } from '../../features/auth/useAuth';
import { LoadingStatus } from '../../shared/ui/LoadingStatus';

/** Route wrapper: renders child routes only for a signed-in user. */
export function RequireAuth() {
  const { status } = useAuth();
  const location = useLocation();

  if (status === 'loading') return <LoadingStatus />;
  if (status === 'anonymous') {
    return <Navigate to="/sign-in" replace state={{ from: location }} />;
  }
  return <Outlet />;
}
