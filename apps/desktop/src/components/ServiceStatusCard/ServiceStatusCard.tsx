import { zhCN } from "../../content/zh-CN";
import type { RuntimeStatus } from "../../types/runtime";
import { StatusBadge } from "../StatusBadge/StatusBadge";

import styles from "./ServiceStatusCard.module.css";

type ServiceStatusCardProps = {
  status: RuntimeStatus;
  onRetry?: () => void;
};

export function ServiceStatusCard({ status, onRetry }: ServiceStatusCardProps) {
  const copy = getStatusCopy(status);

  return (
    <section className={styles.card} aria-labelledby="service-status-title">
      <div className={styles.accent} aria-hidden="true" />
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>{zhCN.service.eyebrow}</p>
          <h2 id="service-status-title" className={styles.title}>
            {zhCN.service.title}
          </h2>
        </div>
        <StatusBadge state={status.state} label={copy.label} />
      </header>

      <div className={styles.body} aria-live="polite">
        <div className={styles.summary}>
          <div className={`${styles.stateMark} ${styles[status.state]}`}>
            <span aria-hidden="true">{getStatusMark(status.state)}</span>
          </div>
          <div>
            <h3 className={styles.stateTitle}>{copy.title}</h3>
            <p className={styles.description}>{copy.description}</p>
          </div>
        </div>

        {status.state === "connected" ? (
          <dl className={styles.details}>
            <RuntimeDetail
              label={zhCN.service.fields.appVersion}
              value={status.appVersion}
            />
            <RuntimeDetail
              label={zhCN.service.fields.appServer}
              value={status.appServerUserAgent}
              code
            />
            <RuntimeDetail
              label={zhCN.service.fields.platform}
              value={status.platform}
            />
            <RuntimeDetail
              label={zhCN.service.fields.codexHome}
              value={status.codexHome}
              code
              wide
            />
          </dl>
        ) : null}

        {status.state === "error" ? (
          <div className={styles.errorPanel} role="alert">
            <p>{status.message}</p>
            <button
              className={styles.retryButton}
              type="button"
              onClick={onRetry}
              disabled={!status.retryable || !onRetry}
            >
              {zhCN.service.error.retry}
            </button>
          </div>
        ) : null}
      </div>
    </section>
  );
}

type RuntimeDetailProps = {
  label: string;
  value: string;
  code?: boolean;
  wide?: boolean;
};

function RuntimeDetail({ label, value, code, wide }: RuntimeDetailProps) {
  return (
    <div className={wide ? styles.detailWide : styles.detail}>
      <dt>{label}</dt>
      <dd className={code ? styles.code : undefined}>{value}</dd>
    </div>
  );
}

function getStatusCopy(status: RuntimeStatus) {
  switch (status.state) {
    case "starting":
      return zhCN.service.starting;
    case "connected":
      return zhCN.service.connected;
    case "error":
      return zhCN.service.error;
    case "stopped":
      return zhCN.service.stopped;
  }
}

function getStatusMark(state: RuntimeStatus["state"]) {
  switch (state) {
    case "starting":
      return "···";
    case "connected":
      return "✓";
    case "error":
      return "!";
    case "stopped":
      return "–";
  }
}
