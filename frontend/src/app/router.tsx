import { Route, Routes } from 'react-router';
import { SignInPage } from '../features/auth/pages/SignInPage';
import { SignUpPage } from '../features/auth/pages/SignUpPage';
import { HomePage } from '../features/home/HomePage';
import { GuestOnly } from './guards/GuestOnly';
import { RequireAuth } from './guards/RequireAuth';
import { AppLayout } from './layout/AppLayout';
import { NotFoundPage } from './NotFoundPage';

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route element={<GuestOnly />}>
          <Route path="sign-in" element={<SignInPage />} />
          <Route path="sign-up" element={<SignUpPage />} />
        </Route>
        <Route element={<RequireAuth />}>
          <Route index element={<HomePage />} />
        </Route>
        {/* Outside both guards: an unknown path never needs a session. */}
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
