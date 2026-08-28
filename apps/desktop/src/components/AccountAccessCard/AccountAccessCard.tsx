import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import { zhCN } from "../../content/zh-CN";
import type { AccountAction } from "../../hooks/useAccountStatus";
import type { AccountStatus } from "../../types/account";

import styles from "./AccountAccessCard.module.css";

type AccountAccessCardProps = {
  runtimeConnected: boolean;
  status: AccountStatus;
  pendingAction: AccountAction | null;
  onRefresh: () => void;
  onStartChatgptLogin: () => void;
  onCancelLogin: () => void;
  onLogout: () => void;
};

type AccountTone = AccountStatus["state"] | "unavailable";

export function AccountAccessCard({
  runtimeConnected,
  status,
  pendingAction,
  onRefresh,
  onStartChatgptLogin,
  onCancelLogin,
  onLogout,
}: AccountAccessCardProps) {
  const [logoutDialogOpen, setLogoutDialogOpen] = useState(false);
  const cancelLogoutRef = useRef<HTMLButtonElement>(null);
  const confirmLogoutRef = useRef<HTMLButtonElement>(null);
  const logoutTriggerRef = useRef<HTMLButtonElement>(null);
  const wasLogoutDialogOpenRef = useRef(false);
  const busy = pendingAction !== null;
  const view = getAccountView(runtimeConnected, status);

  useEffect(() => {
    if (logoutDialogOpen) {
      cancelLogoutRef.current?.focus();
    } else if (wasLogoutDialogOpenRef.current) {
      logoutTriggerRef.current?.focus();
    }
    wasLogoutDialogOpenRef.current = logoutDialogOpen;
  }, [logoutDialogOpen]);

  useEffect(() => {
    if (status.state !== "signedIn") {
      setLogoutDialogOpen(false);
    }
  }, [status.state]);

  const closeLogoutDialog = () => setLogoutDialogOpen(false);
  const handleLogoutDialogKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeLogoutDialog();
    } else if (
      event.key === "Tab" &&
      event.shiftKey &&
      document.activeElement === cancelLogoutRef.current
    ) {
      event.preventDefault();
      confirmLogoutRef.current?.focus();
    } else if (
      event.key === "Tab" &&
      !event.shiftKey &&
      document.activeElement === confirmLogoutRef.current
    ) {
      event.preventDefault();
      cancelLogoutRef.current?.focus();
    }
  };

  return (
    <section className={styles.card} aria-labelledby="account-access-title">
      <div className={styles.accent} aria-hidden="true" />
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>{zhCN.account.eyebrow}</p>
          <h2 id="account-access-title" className={styles.title}>
            {zhCN.account.title}
          </h2>
        </div>
        <AccountBadge tone={view.tone} label={view.label} />
      </header>

      <div
        className={`${styles.body} ${view.tone === "error" ? styles.errorBody : ""}`}
        aria-live="polite"
        role={view.tone === "error" ? "alert" : undefined}
      >
        <AccountSummary {...view} />

        {runtimeConnected && status.state === "signedOut" ? (
          <div className={styles.actions}>
            <AccountButton
              variant="primary"
              busy={busy}
              pending={pendingAction === "startChatgptLogin"}
              onClick={onStartChatgptLogin}
            >
              {zhCN.account.actions.browserLogin}
            </AccountButton>
          </div>
        ) : null}

        {runtimeConnected && status.state === "browserPending" ? (
          <>
            <div className={styles.pendingHint}>
              <span aria-hidden="true" />
              {zhCN.account.browserPending.hint}
            </div>
            <div className={styles.actions}>
              <AccountButton
                variant="ghost"
                busy={busy}
                pending={pendingAction === "cancelLogin"}
                onClick={onCancelLogin}
              >
                {zhCN.account.actions.cancel}
              </AccountButton>
            </div>
          </>
        ) : null}

        {runtimeConnected && status.state === "signedIn" ? (
          <>
            <dl className={styles.detailPanel}>
              <div>
                <dt>{zhCN.account.signedIn.emailLabel}</dt>
                <dd>
                  {status.email ?? zhCN.account.signedIn.emailUnavailable}
                </dd>
              </div>
              <div>
                <dt>{zhCN.account.signedIn.planLabel}</dt>
                <dd>{status.planType}</dd>
              </div>
            </dl>
            <div className={styles.actions}>
              <button
                ref={logoutTriggerRef}
                className={styles.dangerButton}
                type="button"
                onClick={() => setLogoutDialogOpen(true)}
                disabled={busy}
              >
                {zhCN.account.actions.logout}
              </button>
            </div>
          </>
        ) : null}

        {runtimeConnected && status.state === "error" ? (
          <div className={styles.actions}>
            <AccountButton
              variant="primary"
              busy={busy || !status.retryable}
              pending={pendingAction === "refresh"}
              onClick={onRefresh}
            >
              {zhCN.account.actions.retry}
            </AccountButton>
          </div>
        ) : null}
      </div>

      {logoutDialogOpen && status.state === "signedIn" ? (
        <div
          className={styles.dialogBackdrop}
          role="presentation"
          onKeyDown={handleLogoutDialogKeyDown}
        >
          <div
            className={styles.dialog}
            role="dialog"
            aria-modal="true"
            aria-labelledby="logout-dialog-title"
            aria-describedby="logout-dialog-description"
          >
            <p className={styles.eyebrow}>
              {zhCN.account.logoutDialog.eyebrow}
            </p>
            <h3 id="logout-dialog-title">{zhCN.account.logoutDialog.title}</h3>
            <p id="logout-dialog-description">
              {zhCN.account.logoutDialog.description}
            </p>
            <div className={styles.dialogActions}>
              <button
                ref={cancelLogoutRef}
                className={styles.secondaryButton}
                type="button"
                onClick={closeLogoutDialog}
              >
                {zhCN.account.logoutDialog.cancel}
              </button>
              <button
                ref={confirmLogoutRef}
                className={styles.dangerButton}
                type="button"
                onClick={() => {
                  closeLogoutDialog();
                  onLogout();
                }}
                disabled={busy}
              >
                {zhCN.account.logoutDialog.confirm}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}

type AccountView = {
  mark: string;
  tone: AccountTone;
  label: string;
  title: string;
  description: string;
};

function AccountSummary({ mark, tone, title, description }: AccountView) {
  return (
    <div className={styles.summary}>
      <div className={`${styles.stateMark} ${styles[tone]}`} aria-hidden="true">
        {mark}
      </div>
      <div>
        <h3 className={styles.stateTitle}>{title}</h3>
        <p className={styles.description}>{description}</p>
      </div>
    </div>
  );
}

function AccountBadge({ tone, label }: Pick<AccountView, "tone" | "label">) {
  return (
    <span className={`${styles.badge} ${styles[tone]}`}>
      <span aria-hidden="true" />
      {label}
    </span>
  );
}

type AccountButtonProps = {
  variant: "primary" | "ghost";
  busy: boolean;
  pending?: boolean;
  onClick: () => void;
  children: ReactNode;
};

function AccountButton({
  variant,
  busy,
  pending,
  onClick,
  children,
}: AccountButtonProps) {
  return (
    <button
      className={styles[`${variant}Button`]}
      type="button"
      onClick={onClick}
      disabled={busy}
      aria-busy={pending || undefined}
    >
      {children}
    </button>
  );
}

function getAccountView(
  runtimeConnected: boolean,
  status: AccountStatus,
): AccountView {
  if (!runtimeConnected) {
    return {
      mark: "—",
      tone: "unavailable",
      ...zhCN.account.runtimeUnavailable,
    };
  }

  switch (status.state) {
    case "checking":
      return { mark: "···", tone: status.state, ...zhCN.account.checking };
    case "signedOut":
      return { mark: "↗", tone: status.state, ...zhCN.account.signedOut };
    case "browserPending":
      return {
        mark: "↗",
        tone: status.state,
        ...zhCN.account.browserPending,
      };
    case "signedIn":
      return { mark: "✓", tone: status.state, ...zhCN.account.signedIn };
    case "error":
      return {
        mark: "!",
        tone: status.state,
        label: zhCN.account.error.label,
        title: zhCN.account.error.title,
        description: status.message,
      };
  }
}
