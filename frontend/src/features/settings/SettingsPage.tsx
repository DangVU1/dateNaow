import { useMutation } from '@tanstack/react-query';
import { useId } from 'react';
import { Link } from 'react-router';
import type { Gender } from '../../shared/api/types';
import { Button } from '../../shared/ui/Button';
import { FormMessage } from '../../shared/ui/FormMessage';
import { anyErrorMessage } from '../auth/messages';
import { useAuth } from '../auth/useAuth';
import { updateGender } from './api';
import styles from './SettingsPage.module.css';

const GENDER_OPTIONS: readonly { value: Gender; label: string }[] = [
  { value: 'male', label: 'Nam' },
  { value: 'female', label: 'Nữ' },
  { value: 'other', label: 'Khác' },
  { value: 'prefer_not_to_say', label: 'Không muốn nói' },
];

export function SettingsPage() {
  const { user, setUser } = useAuth();
  const mutation = useMutation({
    mutationFn: (gender: Gender | null) => updateGender(gender),
    onSuccess: setUser,
  });

  // While saving, show the choice being sent; otherwise the saved one, so a
  // failed save falls back to what the server has.
  const selected = mutation.isPending
    ? mutation.variables
    : (user?.gender ?? null);
  const pending = mutation.isPending;
  const noteId = useId();

  const choose = (gender: Gender | null) => {
    if (pending) return;
    mutation.mutate(gender);
  };

  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Cài đặt</h1>
      {/* Not `disabled` while saving: that would drop keyboard focus from the
          radio. Changes made while a save is pending are ignored in `choose`. */}
      <fieldset
        className={styles.group}
        aria-busy={pending}
        aria-describedby={noteId}
      >
        <legend className={styles.legend}>Giới tính</legend>
        <p id={noteId} className={styles.note}>
          Mèo dùng mục này để biết nên nhắc bạn vào ngày lễ nào. Người ấy không
          bao giờ thấy mục này.
        </p>
        <div className={styles.options}>
          {GENDER_OPTIONS.map((option) => (
            <label key={option.value} className={styles.option}>
              <input
                type="radio"
                name="gender"
                value={option.value}
                checked={selected === option.value}
                onChange={() => {
                  choose(option.value);
                }}
              />
              {option.label}
            </label>
          ))}
        </div>
        <Button
          variant="secondary"
          disabled={pending || selected === null}
          onClick={() => {
            choose(null);
          }}
        >
          Bỏ chọn
        </Button>
      </fieldset>
      <FormMessage
        message={
          mutation.error === null ? null : anyErrorMessage(mutation.error)
        }
      />
      <p>
        <Link to="/" className={styles.link}>
          Về trang chủ
        </Link>
      </p>
    </section>
  );
}
