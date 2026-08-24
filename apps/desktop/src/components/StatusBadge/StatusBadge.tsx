import type { RuntimeStatus } from "../../types/runtime";

import styles from "./StatusBadge.module.css";

type StatusBadgeProps = {
  state: RuntimeStatus["state"];
  label: string;
};

export function StatusBadge({ state, label }: StatusBadgeProps) {
  return (
    <span className={`${styles.badge} ${styles[state]}`}>
      <span className={styles.indicator} aria-hidden="true" />
      <span>{label}</span>
    </span>
  );
}
