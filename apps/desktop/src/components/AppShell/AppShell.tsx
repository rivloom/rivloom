import type { ReactNode } from "react";

import { zhCN } from "../../content/zh-CN";
import type { RuntimeStatus } from "../../types/runtime";
import { StatusBadge } from "../StatusBadge/StatusBadge";

import styles from "./AppShell.module.css";

type AppShellProps = {
  runtimeStatus: RuntimeStatus;
  stage?: { title: string; description: string };
  children: ReactNode;
};

export function AppShell({ runtimeStatus, stage, children }: AppShellProps) {
  const statusCopy = getStatusBarCopy(runtimeStatus.state);
  const stageCopy = stage ?? {
    title: zhCN.navigation.stageTitle,
    description: zhCN.navigation.stageDescription,
  };

  return (
    <div className={styles.shell}>
      <header className={styles.topBar}>
        <div className={styles.brand}>
          <span className={styles.brandMark} aria-hidden="true">
            R
          </span>
          <div className={styles.brandName}>
            <h1>{zhCN.product.name}</h1>
            <span>{zhCN.product.edition}</span>
          </div>
        </div>
        <span className={styles.workspace}>{zhCN.product.workspace}</span>
      </header>

      <aside className={styles.sidebar}>
        <nav aria-label={zhCN.navigation.label}>
          <a
            className={styles.activeNavItem}
            href="#overview"
            aria-current="page"
          >
            <span className={styles.navMarker} aria-hidden="true" />
            {zhCN.navigation.overview}
          </a>
        </nav>

        <section className={styles.stageNote} aria-labelledby="stage-title">
          <p>{zhCN.navigation.stageLabel}</p>
          <h2 id="stage-title">{stageCopy.title}</h2>
          <span>{stageCopy.description}</span>
        </section>
      </aside>

      <main id="overview" className={styles.main} tabIndex={-1}>
        {children}
      </main>

      <footer className={styles.statusBar} aria-label={zhCN.statusBar.label}>
        <StatusBadge state={runtimeStatus.state} label={statusCopy.badge} />
        <span className={styles.statusText}>{statusCopy.description}</span>
      </footer>
    </div>
  );
}

function getStatusBarCopy(state: RuntimeStatus["state"]) {
  switch (state) {
    case "starting":
      return {
        badge: zhCN.service.starting.label,
        description: zhCN.statusBar.starting,
      };
    case "connected":
      return {
        badge: zhCN.service.connected.label,
        description: zhCN.statusBar.connected,
      };
    case "error":
      return {
        badge: zhCN.service.error.label,
        description: zhCN.statusBar.error,
      };
    case "stopped":
      return {
        badge: zhCN.service.stopped.label,
        description: zhCN.statusBar.stopped,
      };
  }
}
