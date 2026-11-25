import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { refreshAccessToken } from '../../../shared/api/client';
import { ApiError } from '../../../shared/api/errors';
import { getAccessToken, setAccessToken } from '../../../shared/api/tokenStore';
import { authResponse } from '../../../test/fixtures';
import { currentLocation, renderApp } from '../../../test/renderApp';
import { register } from '../api';

vi.mock('../api', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
}));

vi.mock('../../../shared/api/client', () => ({
  refreshAccessToken: vi.fn(),
  onSessionExpired: vi.fn(() => () => {}),
  waitForPendingRefresh: vi.fn(() => Promise.resolve()),
}));

beforeEach(() => {
  vi.mocked(refreshAccessToken).mockResolvedValue(null);
  setAccessToken(null);
});

afterEach(() => {
  vi.clearAllMocks();
});

async function fillAndSubmit(email: string, password: string) {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('Email'), email);
  await user.type(screen.getByLabelText('Mật khẩu'), password);
  await user.click(screen.getByRole('button', { name: 'Tạo tài khoản' }));
}

test('SignUpPage shows field messages from server 422 by field and code', async () => {
  vi.mocked(register).mockRejectedValue(
    new ApiError(422, 'validation_error', undefined, {
      fields: { email: ['invalid_email'], password: ['too_long'] },
    }),
  );
  renderApp('/sign-up');
  await fillAndSubmit('an@', 'mat-khau-dai');

  const emailError = await screen.findByText('Email chưa đúng định dạng.');
  const email = screen.getByLabelText('Email');
  expect(email).toHaveAttribute('aria-invalid', 'true');
  expect(email.getAttribute('aria-describedby')).toContain(emailError.id);
  expect(
    screen.getByText('Mật khẩu dài tối đa 128 ký tự.'),
  ).toBeInTheDocument();
  // Field errors replace the generic form-level message.
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

test('SignUpPage shows email_taken message', async () => {
  vi.mocked(register).mockRejectedValue(new ApiError(409, 'email_taken'));
  renderApp('/sign-up');
  await fillAndSubmit('an@example.com', 'mat-khau-dai');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Email này đã có tài khoản. Bạn đăng nhập nhé.',
  );
});

test('SignUpPage rejects a password over 128 characters without calling the api', async () => {
  renderApp('/sign-up');
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('Email'), 'an@example.com');
  await user.click(screen.getByLabelText('Mật khẩu'));
  await user.paste('a'.repeat(129));
  await user.click(screen.getByRole('button', { name: 'Tạo tài khoản' }));

  expect(
    await screen.findByText('Mật khẩu dài tối đa 128 ký tự.'),
  ).toBeInTheDocument();
  expect(register).not.toHaveBeenCalled();
});

test('SignUpPage signs up and lands on home greeting the user', async () => {
  vi.mocked(register).mockResolvedValue(
    authResponse('moi@example.com', 'tok-new'),
  );
  renderApp('/sign-up');
  await fillAndSubmit('moi@example.com', 'mật khẩu có dấu');

  expect(
    await screen.findByRole('heading', { name: 'Xin chào, moi@example.com' }),
  ).toBeInTheDocument();
  expect(currentLocation()).toBe('/');
  expect(register).toHaveBeenCalledWith('moi@example.com', 'mật khẩu có dấu');
  expect(getAccessToken()).toBe('tok-new');
});
