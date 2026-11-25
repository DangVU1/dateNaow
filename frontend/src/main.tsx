import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router';
import { AppProviders } from './app/providers';
import { AppRoutes } from './app/router';
import './shared/styles/global.css';

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Root element #root not found');
}

createRoot(rootElement).render(
  <StrictMode>
    <BrowserRouter>
      <AppProviders>
        <AppRoutes />
      </AppProviders>
    </BrowserRouter>
  </StrictMode>,
);
