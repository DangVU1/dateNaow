import { useMutation } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { ApiError, type FieldErrors } from '../../../shared/api/errors';
import { Button } from '../../../shared/ui/Button';
import { FormMessage } from '../../../shared/ui/FormMessage';
import { TextField } from '../../../shared/ui/TextField';
import { anyErrorMessage, fieldMessage } from '../messages';
import { validateCredentials } from '../validation';
import styles from './AuthPage.module.css';

interface Credentials {
  email: string;
  password: string;
}

export interface CredentialsFormProps {
  submitLabel: string;
  passwordAutoComplete: 'current-password' | 'new-password';
  onSubmit: (email: string, password: string) => Promise<void>;
}

function hasErrors(fields: FieldErrors): boolean {
  return Object.keys(fields).length > 0;
}

/** Email + password form shared by the sign-in and sign-up pages. */
export function CredentialsForm({
  submitLabel,
  passwordAutoComplete,
  onSubmit,
}: CredentialsFormProps) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [clientErrors, setClientErrors] = useState<FieldErrors>({});
  const mutation = useMutation({
    mutationFn: ({ email, password }: Credentials) => onSubmit(email, password),
  });

  const error = mutation.error;
  const serverFields: FieldErrors =
    error instanceof ApiError && error.status === 422
      ? (error.fields ?? {})
      : {};
  const fields = hasErrors(clientErrors) ? clientErrors : serverFields;
  // A 422 with codes for the visible fields is shown next to them instead.
  const shownOnFields =
    (serverFields.email?.length ?? 0) > 0 ||
    (serverFields.password?.length ?? 0) > 0;
  const formError =
    error !== null && !shownOnFields ? anyErrorMessage(error) : null;

  const messageFor = (field: string): string | undefined => {
    const code = fields[field]?.[0];
    return code === undefined ? undefined : fieldMessage(field, code);
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (mutation.isPending) return;
    const errors = validateCredentials(email, password);
    setClientErrors(errors);
    if (hasErrors(errors)) {
      mutation.reset();
      return;
    }
    mutation.mutate({ email, password });
  };

  return (
    // Native validation is off so every message is the Vietnamese one.
    <form className={styles.form} onSubmit={handleSubmit} noValidate>
      <TextField
        label="Email"
        name="email"
        type="email"
        inputMode="email"
        autoComplete="email"
        autoCapitalize="none"
        spellCheck={false}
        value={email}
        onChange={(e) => {
          setEmail(e.target.value);
        }}
        error={messageFor('email')}
      />
      <TextField
        label="Mật khẩu"
        name="password"
        type="password"
        autoComplete={passwordAutoComplete}
        value={password}
        onChange={(e) => {
          setPassword(e.target.value);
        }}
        error={messageFor('password')}
      />
      <FormMessage message={formError} />
      <Button type="submit" disabled={mutation.isPending}>
        {submitLabel}
      </Button>
    </form>
  );
}
