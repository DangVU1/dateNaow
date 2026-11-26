import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState, type ReactNode } from 'react';
import { AuthProvider } from '../features/auth/AuthProvider';

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: 1 },
      mutations: { retry: 0 },
    },
  });
}

export function AppProviders({ children }: { children: ReactNode }) {
  const [queryClient] = useState(createQueryClient);
  return (
    // AuthProvider sits inside so sign-out can clear the query cache.
    <QueryClientProvider client={queryClient}>
      <AuthProvider>{children}</AuthProvider>
    </QueryClientProvider>
  );
}
