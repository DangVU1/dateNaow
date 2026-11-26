import { useLocation } from 'react-router';

/** Shows the current path so tests can assert where navigation ended. */
export function LocationProbe() {
  const location = useLocation();
  return (
    <output data-testid="location" hidden>
      {location.pathname + location.search}
    </output>
  );
}
