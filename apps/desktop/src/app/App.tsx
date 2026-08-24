import { AppShell } from "../components/AppShell/AppShell";
import { ServiceStatusCard } from "../components/ServiceStatusCard/ServiceStatusCard";
import { zhCN } from "../content/zh-CN";
import { useRuntimeStatus } from "../hooks/useRuntimeStatus";

import styles from "./App.module.css";

export function App() {
  const { retry, retrying, status } = useRuntimeStatus();

  return (
    <AppShell runtimeStatus={status}>
      <div className={styles.page}>
        <section className={styles.intro} aria-labelledby="overview-title">
          <div>
            <p className={styles.eyebrow}>{zhCN.overview.eyebrow}</p>
            <h2 id="overview-title">{zhCN.overview.title}</h2>
            <p className={styles.description}>{zhCN.overview.description}</p>
          </div>

          <aside className={styles.privacyNote}>
            <span className={styles.privacyMark} aria-hidden="true">
              ↘
            </span>
            <div>
              <strong>{zhCN.overview.privacyLabel}</strong>
              <p>{zhCN.overview.privacyDescription}</p>
            </div>
          </aside>
        </section>

        <ServiceStatusCard
          status={status}
          onRetry={retry}
          retrying={retrying}
        />
      </div>
    </AppShell>
  );
}
