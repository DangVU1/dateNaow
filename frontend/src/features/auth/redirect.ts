function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/**
 * Where to go after signing in: `state.from` set by `RequireAuth`, as
 * pathname + search, else `/`. History state is under the page's control,
 * so only same-app paths are accepted: they must start with a single `/`
 * (`//host` and `/\host` would leave the site) and not point back to the
 * sign-in pages.
 */
export function redirectTarget(state: unknown): string {
  if (!isRecord(state) || !isRecord(state.from)) return '/';
  const { pathname, search } = state.from;
  if (typeof pathname !== 'string' || !pathname.startsWith('/')) return '/';
  if (pathname.startsWith('//') || pathname.startsWith('/\\')) return '/';
  if (pathname === '/sign-in' || pathname === '/sign-up') return '/';
  const query =
    typeof search === 'string' && search.startsWith('?') ? search : '';
  return pathname + query;
}
