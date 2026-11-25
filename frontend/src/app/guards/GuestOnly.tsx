import { Navigate, Outlet, useLocation } from 'react-router';
import { redirectTarget } from '../../features/auth/redirect';
import { useAuth } from '../../features/auth/useAuth';
import { LoadingStatus } from '../../shared/ui/LoadingStatus';

/**
 * Route wrapper for the sign-in and sign-up pages. A signed-in user is sent
 * to the page they came from (`state.from`) or home. This is also how a
 * successful sign-in or sign-up navigates: the status turns authenticated
 * and this guard redirects, so there is a single place that decides.
 */
export function GuestOnly() {
  const { status } = useAuth();
  const location = useLocation();

  if (status === 'loading') return <LoadingStatus />;
  if (status === 'authenticated') {
    return <Navigate to={redirectTarget(location.state)} replace />;
  }
  return <Outlet />;
}
