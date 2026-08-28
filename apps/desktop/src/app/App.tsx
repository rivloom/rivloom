import { useEffect, useState } from "react";

import { AccountAccessCard } from "../components/AccountAccessCard/AccountAccessCard";
import { AppShell } from "../components/AppShell/AppShell";
import { ProjectAccessCard } from "../components/ProjectAccessCard/ProjectAccessCard";
import { ProjectWorkspace } from "../components/ProjectWorkspace/ProjectWorkspace";
import { ServiceStatusCard } from "../components/ServiceStatusCard/ServiceStatusCard";
import { zhCN } from "../content/zh-CN";
import { useAccountStatus } from "../hooks/useAccountStatus";
import { useRecentProjects } from "../hooks/useRecentProjects";
import { useRuntimeStatus } from "../hooks/useRuntimeStatus";
import type { LocalProject } from "../types/project";

import styles from "./App.module.css";

export function App() {
  const { retry, retrying, status: runtimeStatus } = useRuntimeStatus();
  const runtimeConnected = runtimeStatus.state === "connected";
  const account = useAccountStatus(runtimeConnected);
  const signedIn = account.status.state === "signedIn";
  const projectAccessReady = runtimeConnected && signedIn;
  const projects = useRecentProjects(projectAccessReady);
  const [activeProject, setActiveProject] = useState<LocalProject | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [lastConfirmedSignedIn, setLastConfirmedSignedIn] = useState(false);
  const disconnectedSession =
    !runtimeConnected &&
    lastConfirmedSignedIn &&
    account.status.state !== "signedOut";
  const showWorkspace =
    (signedIn || disconnectedSession) &&
    activeProject !== null &&
    workspaceOpen;
  const projectExperience = projectAccessReady || showWorkspace;
  const overview = projectAccessReady ? zhCN.projectOverview : zhCN.overview;

  useEffect(() => {
    if (account.status.state === "signedIn") {
      setLastConfirmedSignedIn(true);
    } else if (account.status.state === "signedOut") {
      setLastConfirmedSignedIn(false);
      setWorkspaceOpen(false);
    }
  }, [account.status.state]);

  const openProject = (project: LocalProject) => {
    setActiveProject(project);
    setWorkspaceOpen(true);
  };

  return (
    <AppShell
      runtimeStatus={runtimeStatus}
      stage={projectExperience ? zhCN.navigation.projectStage : undefined}
    >
      {showWorkspace ? (
        <ProjectWorkspace
          project={activeProject}
          runtimeConnected={runtimeConnected}
          onBack={() => setWorkspaceOpen(false)}
        />
      ) : (
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
              runtimeConnected={runtimeConnected}
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
                activeProjectId={activeProject?.id ?? null}
                onRefresh={projects.refresh}
                onSelect={projects.select}
                onOpenProject={openProject}
                onRemove={projects.remove}
              />
            ) : null}
          </div>
        </div>
      )}
    </AppShell>
  );
}
