import styles from './LoadingStatus.module.css';

export function LoadingStatus() {
  return (
    <p role="status" className={styles.status}>
      Đang tải…
    </p>
  );
}
