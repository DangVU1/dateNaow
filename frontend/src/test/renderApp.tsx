import { render } from '@testing-library/react';
import { StrictMode, type ReactNode } from 'react';
import { MemoryRouter, type MemoryRouterProps } from 'react-router';
import { AppProviders } from '../app/providers';
import { AppRoutes } from '../app/router';
import { LocationProbe } from './LocationProbe';

type InitialEntry = NonNullable<MemoryRouterProps['initialEntries']>[number];

export interface RenderAppOptions {
  /** Wrap in `<StrictMode>` like `main.tsx` does (effects run twice). */
  strictMode?: boolean;
}

export function renderApp(
  entry: InitialEntry,
  { strictMode = false }: RenderAppOptions = {},
) {
  const app: ReactNode = (
    <MemoryRouter initialEntries={[entry]}>
      <AppProviders>
        <AppRoutes />
      </AppProviders>
      <LocationProbe />
    </MemoryRouter>
  );
  return render(strictMode ? <StrictMode>{app}</StrictMode> : app);
}

/** The path (with search) the router is on, as shown by `LocationProbe`. */
export function currentLocation(): string | null {
  return (
    document.querySelector('[data-testid="location"]')?.textContent ?? null
  );
}
