import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { refreshAccessToken } from '../../shared/api/client';
import { ApiError } from '../../shared/api/errors';
import { setAccessToken } from '../../shared/api/tokenStore';
import type { Gender } from '../../shared/api/types';
import { authResponse } from '../../test/fixtures';
import { renderApp } from '../../test/renderApp';
import { updateGender } from './api';

vi.mock('./api', () => ({
  updateGender: vi.fn(),
}));

vi.mock('../auth/api', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
}));

vi.mock('../../shared/api/client', () => ({
  refreshAccessToken: vi.fn(),
  onSessionExpired: vi.fn(() => () => {}),
  waitForPendingRefresh: vi.fn(() => Promise.resolve()),
}));

function signedInAs(gender: Gender | null) {
  vi.mocked(refreshAccessToken).mockResolvedValue(
    authResponse('an@example.com', 'tok-1', { gender }),
  );
}

function savedAs(gender: Gender | null) {
  vi.mocked(updateGender).mockResolvedValue(
    authResponse('an@example.com', 'tok-1', { gender }).user,
  );
}

beforeEach(() => {
  setAccessToken(null);
});

afterEach(() => {
  vi.clearAllMocks();
});

test('SettingsPage selecting Nữ sends female and shows it as selected', async () => {
  signedInAs(null);
  savedAs('female');
  renderApp('/settings');

  const group = await screen.findByRole('group', { name: 'Giới tính' });
  expect(group).toBeInTheDocument();
  expect(
    screen.getByText(
      'Mèo dùng mục này để biết nên nhắc bạn vào ngày lễ nào. Người ấy không bao giờ thấy mục này.',
    ),
  ).toBeInTheDocument();
  for (const name of ['Nam', 'Nữ', 'Khác', 'Không muốn nói']) {
    expect(screen.getByRole('radio', { name })).not.toBeChecked();
  }

  await userEvent.setup().click(screen.getByRole('radio', { name: 'Nữ' }));

  // Checked and enabled again only once the save has finished.
  await waitFor(() => {
    const female = screen.getByRole('radio', { name: 'Nữ' });
    expect(female).toBeChecked();
    expect(female).toBeEnabled();
  });
  expect(updateGender).toHaveBeenCalledTimes(1);
  expect(updateGender).toHaveBeenCalledWith('female');
});

test('SettingsPage Bỏ chọn sends null', async () => {
  signedInAs('male');
  savedAs(null);
  renderApp('/settings');

  expect(await screen.findByRole('radio', { name: 'Nam' })).toBeChecked();
  await userEvent
    .setup()
    .click(screen.getByRole('button', { name: 'Bỏ chọn' }));

  await waitFor(() => {
    const male = screen.getByRole('radio', { name: 'Nam' });
    expect(male).not.toBeChecked();
    expect(male).toBeEnabled();
  });
  expect(updateGender).toHaveBeenCalledWith(null);
  // Nothing left to clear.
  expect(screen.getByRole('button', { name: 'Bỏ chọn' })).toBeDisabled();
});

test('SettingsPage ignores other choices while saving and keeps focus', async () => {
  signedInAs(null);
  vi.mocked(updateGender).mockReturnValue(new Promise(() => {}));
  renderApp('/settings');
  const user = userEvent.setup();

  await user.click(await screen.findByRole('radio', { name: 'Khác' }));
  await user.click(screen.getByRole('radio', { name: 'Nam' }));

  expect(screen.getByRole('radio', { name: 'Khác' })).toBeChecked();
  expect(screen.getByRole('radio', { name: 'Nam' })).not.toBeChecked();
  expect(screen.getByRole('radio', { name: 'Nam' })).toBeEnabled();
  expect(screen.getByRole('group', { name: 'Giới tính' })).toHaveAttribute(
    'aria-busy',
    'true',
  );
  expect(screen.getByRole('button', { name: 'Bỏ chọn' })).toBeDisabled();
  expect(updateGender).toHaveBeenCalledTimes(1);
});

test('SettingsPage shows an error and keeps the saved choice when saving fails', async () => {
  signedInAs('male');
  vi.mocked(updateGender).mockRejectedValue(
    new ApiError(500, 'internal_error'),
  );
  renderApp('/settings');

  await userEvent
    .setup()
    .click(await screen.findByRole('radio', { name: 'Không muốn nói' }));

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Đã có lỗi, bạn thử lại sau nhé.',
  );
  expect(screen.getByRole('radio', { name: 'Nam' })).toBeChecked();
  expect(updateGender).toHaveBeenCalledWith('prefer_not_to_say');
  expect(screen.getByRole('link', { name: 'Về trang chủ' })).toHaveAttribute(
    'href',
    '/',
  );
});
