import { Route, Routes } from 'react-router';
import { AppLayout } from './layout/AppLayout';
import { NotFoundPage } from './NotFoundPage';

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
