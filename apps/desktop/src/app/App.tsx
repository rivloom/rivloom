import { AppShell } from "../components/AppShell/AppShell";
import { ServiceStatusCard } from "../components/ServiceStatusCard/ServiceStatusCard";
import { zhCN } from "../content/zh-CN";
import type { RuntimeStatus } from "../types/runtime";

import styles from "./App.module.css";

const initialRuntimeStatus: RuntimeStatus = { state: "starting" };

export function App() {
  return (
    <AppShell runtimeStatus={initialRuntimeStatus}>
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

        <ServiceStatusCard status={initialRuntimeStatus} />
      </div>
    </AppShell>
  );
}
