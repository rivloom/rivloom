import { useState } from "react";

import { AccountAccessCard } from "../components/AccountAccessCard/AccountAccessCard";
import { AppShell } from "../components/AppShell/AppShell";
import { ProjectAccessCard } from "../components/ProjectAccessCard/ProjectAccessCard";
import { ServiceStatusCard } from "../components/ServiceStatusCard/ServiceStatusCard";
import { zhCN } from "../content/zh-CN";
import { useAccountStatus } from "../hooks/useAccountStatus";
import { useRecentProjects } from "../hooks/useRecentProjects";
import { useRuntimeStatus } from "../hooks/useRuntimeStatus";

import styles from "./App.module.css";

export function App() {
  const { retry, retrying, status: runtimeStatus } = useRuntimeStatus();
  const account = useAccountStatus(runtimeStatus.state === "connected");
  const projectAccessReady =
    runtimeStatus.state === "connected" && account.status.state === "signedIn";
  const projects = useRecentProjects(projectAccessReady);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const overview = projectAccessReady ? zhCN.projectOverview : zhCN.overview;

  return (
    <AppShell
      runtimeStatus={runtimeStatus}
      stage={projectAccessReady ? zhCN.navigation.projectStage : undefined}
    >
      <div className={styles.page}>
        <section className={styles.intro} aria-labelledby="overview-title">
          <div>
            <p className={styles.eyebrow}>{overview.eyebrow}</p>
            <h2 id="overview-title">{overview.title}</h2>
            <p className={styles.description}>{overview.description}</p>
          </div>

          <aside className={styles.privacyNote}>
            <span className={styles.privacyMark} aria-hidden="true">
              ↘
            </span>
            <div>
              <strong>{overview.privacyLabel}</strong>
              <p>{overview.privacyDescription}</p>
            </div>
          </aside>
        </section>

        <div className={styles.cards}>
          <ServiceStatusCard
            status={runtimeStatus}
            onRetry={retry}
            retrying={retrying}
          />
          <AccountAccessCard
            runtimeConnected={runtimeStatus.state === "connected"}
            status={account.status}
            pendingAction={account.pendingAction}
            onRefresh={account.refresh}
            onStartChatgptLogin={account.beginChatgptLogin}
            onCancelLogin={account.cancelLogin}
            onLogout={account.logout}
          />
          {projectAccessReady ? (
            <ProjectAccessCard
              state={projects.state}
              pendingAction={projects.pendingAction}
              warning={projects.warning}
              activeProjectId={activeProjectId}
              onRefresh={projects.refresh}
              onSelect={projects.select}
              onOpenProject={(project) => setActiveProjectId(project.id)}
              onRemove={projects.remove}
            />
          ) : null}
        </div>
      </div>
    </AppShell>
  );
}
