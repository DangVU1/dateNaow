import { render } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { AppProviders } from '../app/providers';
import { AppRoutes } from '../app/router';

export function renderApp(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AppProviders>
        <AppRoutes />
      </AppProviders>
    </MemoryRouter>,
  );
}
