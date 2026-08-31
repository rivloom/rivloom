import { useState } from "react";
import type { MemberDirectory, MemberEntry } from "../../types/collaboration";
import styles from "./Collaboration.module.css";

export function MemberPanel({
  directory,
  memberId,
  disabled,
  onRevoke,
}: {
  directory: MemberDirectory;
  memberId: string;
  disabled: boolean;
  onRevoke: (id: string) => Promise<unknown>;
}) {
  const [target, setTarget] = useState<MemberEntry | null>(null);
  const members = directory.entries.filter(
    (entry): entry is MemberEntry => entry.type === "member",
  );
  const owner = members.some(
    (member) => member.memberId === memberId && member.owner && !member.revoked,
  );
  const revocable =
    target &&
    owner &&
    members.some(
      (member) =>
        member.memberId === target.memberId && !member.owner && !member.revoked,
    );
  return (
    <section className={styles.panel} aria-labelledby="members-title">
      <div className={styles.row}>
        <h3 id="members-title">成员与 Node</h3>
        <span className={styles.status}>目录修订 {directory.revision}</span>
      </div>
      <p className={styles.note}>
        以下是最后完整对账结果，不代表持续在线。权限由 Brain
        校验；刷新不会重新加入。
      </p>
      <ul className={styles.members}>
        {members.map((member) => (
          <li key={member.memberId}>
            <div className={styles.row}>
              <strong>
                {member.displayName} ·{" "}
                {member.revoked ? "已撤销" : member.owner ? "Owner" : "成员"}
              </strong>
              {owner && !member.owner && !member.revoked ? (
                <button
                  className={styles.button + " " + styles.danger}
                  disabled={disabled}
                  onClick={() => setTarget(member)}
                >
                  撤销 {member.displayName}
                </button>
              ) : null}
            </div>
            <p className={styles.note}>{member.memberId}</p>
            {directory.entries
              .filter(
                (entry) =>
                  entry.type === "node" && entry.memberId === member.memberId,
              )
              .map((entry) =>
                entry.type === "node" ? (
                  <p className={styles.note} key={entry.nodeId}>
                    {entry.nodeId} · 上次对账：{entry.online ? "在线" : "离线"}
                  </p>
                ) : null,
              )}
          </li>
        ))}
      </ul>
      {revocable ? (
        <div role="group" aria-label="确认撤销成员" className={styles.notice}>
          <p>
            {"确认撤销 " +
              target.displayName +
              "（" +
              target.memberId +
              "）？该成员的 Node 将失去协作访问权限；此界面不提供恢复成员入口。"}
          </p>
          <div className={styles.actions}>
            <button
              className={styles.button + " " + styles.danger}
              disabled={disabled}
              onClick={() => {
                setTarget(null);
                void onRevoke(target.memberId);
              }}
            >
              确认撤销
            </button>
            <button className={styles.button} onClick={() => setTarget(null)}>
              保留成员
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
