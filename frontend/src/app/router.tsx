import { Route, Routes } from 'react-router';
import { AdminHomePage } from '../features/admin/AdminHomePage';
import { ForgotPasswordPage } from '../features/auth/pages/ForgotPasswordPage';
import { ResetPasswordPage } from '../features/auth/pages/ResetPasswordPage';
import { SignInPage } from '../features/auth/pages/SignInPage';
import { SignUpPage } from '../features/auth/pages/SignUpPage';
import { HomePage } from '../features/home/HomePage';
import { SettingsPage } from '../features/settings/SettingsPage';
import { GuestOnly } from './guards/GuestOnly';
import { RequireAdmin } from './guards/RequireAdmin';
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
        {/* Outside both guards: a reset link must work whether or not this
            browser is signed in, and its expired message links to
            forgot-password, which must not bounce a signed-in user home. */}
        <Route path="forgot-password" element={<ForgotPasswordPage />} />
        <Route path="reset-password" element={<ResetPasswordPage />} />
        <Route element={<RequireAuth />}>
          <Route index element={<HomePage />} />
          <Route path="settings" element={<SettingsPage />} />
          <Route element={<RequireAdmin />}>
            <Route path="admin" element={<AdminHomePage />} />
          </Route>
        </Route>
        {/* Outside both guards: an unknown path never needs a session. */}
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
