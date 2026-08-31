import { zhCN } from "../../content/zh-CN";
import type { IdentityAction, IdentityState } from "../../hooks/useIdentity";
import type { RivloomIdentity } from "../../types/identity";

import styles from "./IdentityCard.module.css";

type IdentityCardProps = {
  state: IdentityState;
  pendingAction: IdentityAction | null;
  onRefresh: () => void;
};

export function IdentityCard({
  state,
  pendingAction,
  onRefresh,
}: IdentityCardProps) {
  const identity = state.state === "ready" ? state.identity : null;
  return (
    <section className={styles.card} aria-labelledby="identity-card-title">
      <div className={styles.accent} aria-hidden="true" />
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>{zhCN.identity.eyebrow}</p>
          <h2 id="identity-card-title" className={styles.title}>
            {zhCN.identity.title}
          </h2>
        </div>
        <IdentityBadge state={state} />
      </header>

      <div className={styles.body} aria-live="polite">
        {state.state === "loading" ? (
          <StateMessage mark="···" title={zhCN.identity.loading.title} />
        ) : null}
        {state.state === "error" ? (
          <div className={styles.errorPanel} role="alert">
            <StateMessage mark="!" title={state.message} />
            <button
              className={styles.retryButton}
              type="button"
              onClick={onRefresh}
              disabled={pendingAction !== null}
              aria-busy={pendingAction === "refresh" || undefined}
            >
              {zhCN.identity.actions.retry}
            </button>
          </div>
        ) : null}
        {identity ? <IdentitySummary identity={identity} /> : null}
      </div>
    </section>
  );
}

function IdentityBadge({ state }: { state: IdentityState }) {
  const joined = state.state === "ready" && state.identity.brainMembership;
  const tone = joined
    ? "brain"
    : state.state === "ready"
      ? "local"
      : state.state;
  const label = joined
    ? zhCN.identity.badge.brain
    : state.state === "ready"
      ? zhCN.identity.badge.local
      : zhCN.identity.badge[state.state];
  return (
    <span className={`${styles.badge} ${styles[tone]}`}>
      <span aria-hidden="true" />
      {label}
    </span>
  );
}

function StateMessage({ mark, title }: { mark: string; title: string }) {
  return (
    <div className={styles.stateMessage}>
      <span aria-hidden="true">{mark}</span>
      <p>{title}</p>
    </div>
  );
}

function IdentitySummary({ identity }: { identity: RivloomIdentity }) {
  const membership = identity.brainMembership;
  return (
    <>
      <div className={styles.summary}>
        <div>
          <h3>{identity.displayName}</h3>
          <p>
            {membership
              ? zhCN.identity.brain.description
              : zhCN.identity.local.description}
          </p>
        </div>
      </div>
      <dl className={styles.details}>
        <IdentityDetail
          label={zhCN.identity.fields.brain}
          value={
            membership
              ? zhCN.identity.brain.joined
              : zhCN.identity.local.unjoined
          }
        />
        {membership ? (
          <IdentityDetail
            label={zhCN.identity.fields.role}
            value={zhCN.identity.roles[membership.role]}
          />
        ) : null}
        <IdentityDetail
          label={zhCN.identity.fields.deviceId}
          value={identity.deviceId}
          code
        />
        <IdentityDetail
          label={zhCN.identity.fields.identityId}
          value={identity.identityId}
          code
        />
      </dl>
    </>
  );
}

function IdentityDetail({
  label,
  value,
  code = false,
}: {
  label: string;
  value: string;
  code?: boolean;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd className={code ? styles.code : undefined}>{value}</dd>
    </div>
  );
}
