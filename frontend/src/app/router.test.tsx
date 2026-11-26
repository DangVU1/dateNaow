import { screen } from '@testing-library/react';
import { renderApp } from '../test/renderApp';

test('renders not-found page for unknown route', () => {
  renderApp('/khong-co');
  expect(
    screen.getByRole('heading', { name: 'Không tìm thấy trang' }),
  ).toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Về trang chủ' })).toHaveAttribute(
    'href',
    '/',
  );
});
