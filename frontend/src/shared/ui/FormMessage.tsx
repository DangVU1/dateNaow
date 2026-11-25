import styles from './FormMessage.module.css';

/** Form-level error, announced by screen readers when it appears. */
export function FormMessage({ message }: { message: string | null }) {
  if (message === null) return null;
  return (
    <p role="alert" className={styles.message}>
      {message}
    </p>
  );
}
