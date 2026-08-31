import { useState } from "react";
import { collaborationErrors, nodeLabels } from "../../content/collaboration";
import type { CollaborationController } from "../../hooks/useCollaboration";
import { HostBrainPanel } from "./HostBrainPanel";
import { InvitationPanel } from "./InvitationPanel";
import { JoinBrainForm } from "./JoinBrainForm";
import { MemberPanel } from "./MemberPanel";
import { TrustSummary } from "./TrustSummary";
import styles from "./Collaboration.module.css";

export function CollaborationCard({
  controller,
  identityReady,
}: {
  controller: CollaborationController;
  identityReady: boolean;
}) {
  const [mode, setMode] = useState<"join" | "host">("join");
  const { snapshot, pending, error } = controller;
  const busy = pending !== null;
  const disabled = busy || !identityReady;
  const node = snapshot?.node;
  const owner = snapshot?.directory?.entries.some(
    (entry) =>
      entry.type === "member" &&
      entry.memberId === node?.binding?.memberId &&
      entry.owner &&
      !entry.revoked,
  );
  const hostPanel = snapshot ? (
    <HostBrainPanel
      status={snapshot.host}
      nodeState={snapshot.node.state}
      disabled={disabled}
      onInitialize={controller.initialize}
      onStart={controller.start}
      onStop={controller.stop}
      onOwner={controller.owner}
    />
  ) : null;

  return (
    <section
      className={styles.card}
      aria-labelledby="collaboration-title"
      aria-busy={busy}
    >
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>02 / Brain collaboration</p>
          <h2 id="collaboration-title">Brain 协作</h2>
        </div>
        <button
          className={styles.button}
          disabled={!identityReady || busy}
          onClick={() => void controller.reload()}
        >
          读取协作状态
        </button>
      </header>
      <div className={styles.body}>
        <p className={styles.note}>
          成员身份与连接由 Rivloom 管理，不需要 Codex Runtime
          登录。本机目前只支持一个 Brain 登记。
        </p>
        <p className={styles.notice}>
          R3 开发验证：两台真实 Windows
          设备验收尚未通过。当前界面不提供远端任务委派。
        </p>
        {!identityReady ? (
          <p className={styles.status}>
            请先恢复本机 Rivloom 身份，再操作协作连接。
          </p>
        ) : null}
        {identityReady && !snapshot && !error ? (
          <p className={styles.status}>正在读取协作状态…</p>
        ) : null}
        {error ? (
          <p role="alert" className={styles.error}>
            {collaborationErrors[error]}
          </p>
        ) : null}
        {pending ? (
          <p role="status" className={styles.status}>
            正在处理协作操作；请勿重复提交。
          </p>
        ) : null}
        {node ? (
          <div>
            <h3>{nodeLabels[node.state]}</h3>
            {node.registration ? (
              <TrustSummary
                descriptor={node.registration.descriptor}
                fingerprint={node.registration.confirmedFingerprint}
              />
            ) : null}
            {node.state === "recoveryRequired" ? (
              <p className={styles.error}>
                登记已保留，需要显式恢复。恢复流程尚未开放；不会覆盖登记或重发加入。
              </p>
            ) : null}
            {node.state === "connected" ? (
              <p className={styles.note}>
                最后完整对账修订：{node.revision}
                。此状态不是持续在线保证；请手动刷新确认连接。
              </p>
            ) : null}
            <div className={styles.actions}>
              {node.state === "disconnected" ? (
                <button
                  className={styles.button + " " + styles.primary}
                  disabled={disabled}
                  onClick={() => void controller.connect()}
                >
                  连接已登记 Brain
                </button>
              ) : null}
              {node.state === "connected" ? (
                <>
                  <button
                    className={styles.button}
                    disabled={disabled}
                    onClick={() => void controller.refresh()}
                  >
                    刷新连接与目录
                  </button>
                  <button
                    className={styles.button}
                    disabled={disabled}
                    onClick={() => void controller.disconnect()}
                  >
                    断开连接
                  </button>
                </>
              ) : null}
            </div>
          </div>
        ) : null}
        {snapshot && node?.state === "notConfigured" ? (
          <>
            {snapshot.host.state === "notConfigured" ? (
              <>
                <div className={styles.actions} aria-label="协作接入方式">
                  <button
                    className={styles.button}
                    aria-pressed={mode === "join"}
                    disabled={disabled}
                    onClick={() => setMode("join")}
                  >
                    加入 Brain
                  </button>
                  <button
                    className={styles.button}
                    aria-pressed={mode === "host"}
                    disabled={disabled}
                    onClick={() => setMode("host")}
                  >
                    托管 Brain
                  </button>
                </div>
                {mode === "join" ? (
                  <JoinBrainForm disabled={disabled} onJoin={controller.join} />
                ) : (
                  hostPanel
                )}
              </>
            ) : (
              hostPanel
            )}
          </>
        ) : null}
        {snapshot &&
        node?.state !== "notConfigured" &&
        snapshot.host.state !== "notConfigured" ? (
          <details>
            <summary>本机 Brain 托管设置</summary>
            {hostPanel}
          </details>
        ) : null}
        {node?.state === "connected" && snapshot?.directory && node.binding ? (
          <>
            <MemberPanel
              directory={snapshot.directory}
              memberId={node.binding.memberId}
              disabled={disabled}
              onRevoke={controller.revoke}
            />
            {owner ? (
              <InvitationPanel
                disabled={disabled}
                onInvite={controller.invite}
                onCancel={controller.cancel}
              />
            ) : null}
          </>
        ) : null}
      </div>
    </section>
  );
}
