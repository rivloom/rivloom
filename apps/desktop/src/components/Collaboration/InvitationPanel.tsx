import { useEffect, useRef, useState } from "react";
import type { BrainInvitation } from "../../types/collaboration";
import styles from "./Collaboration.module.css";

type Props = {
  disabled: boolean;
  onInvite: () => Promise<BrainInvitation | null>;
  onCancel: (id: string) => Promise<unknown>;
};

export function InvitationPanel({ disabled, onInvite, onCancel }: Props) {
  const [invitation, setInvitation] = useState<BrainInvitation | null>(null);
  const [issued, setIssued] = useState<{
    id: string;
    expiresAt: number;
  } | null>(null);
  const [working, setWorking] = useState(false);
  const [message, setMessage] = useState("");
  const [remaining, setRemaining] = useState(0);
  const active = useRef(false);
  const lock = useRef(false);
  const visibility = useRef(0);
  useEffect(() => {
    active.current = true;
    const hide = () => {
      if (document.hidden) {
        ++visibility.current;
        setInvitation(null);
        setMessage("");
      }
    };
    document.addEventListener("visibilitychange", hide);
    return () => {
      active.current = false;
      ++visibility.current;
      document.removeEventListener("visibilitychange", hide);
    };
  }, []);
  useEffect(() => {
    if (!issued) return;
    const clear = () => {
      setInvitation(null);
      setIssued(null);
      setRemaining(0);
    };
    const tick = () => {
      const seconds = issued.expiresAt - Math.floor(Date.now() / 1000);
      if (seconds <= 0) clear();
      else setRemaining(seconds);
    };
    tick();
    const timer = window.setInterval(tick, 1000);
    // A backwards clock adjustment must not retain the displayed secret indefinitely.
    const limit = window.setTimeout(
      clear,
      Math.min(600000, Math.max(0, issued.expiresAt * 1000 - Date.now())),
    );
    return () => {
      window.clearInterval(timer);
      window.clearTimeout(limit);
    };
  }, [issued]);

  const create = async () => {
    if (disabled || lock.current || issued || document.hidden) return;
    lock.current = true;
    setWorking(true);
    setMessage("");
    const generation = visibility.current;
    try {
      const result = await onInvite();
      if (active.current && result && result.expiresAt > Date.now() / 1000) {
        setIssued({ id: result.invitationId, expiresAt: result.expiresAt });
        if (!document.hidden && generation === visibility.current)
          setInvitation(result);
      }
    } catch {
      if (active.current)
        setMessage("创建结果不确定；未自动重试。请先核对 Brain 状态。");
    } finally {
      lock.current = false;
      if (active.current) setWorking(false);
    }
  };
  const cancel = async () => {
    if (!issued || disabled || lock.current) return;
    lock.current = true;
    setWorking(true);
    setInvitation(null);
    setMessage("");
    try {
      const result = await onCancel(issued.id);
      if (active.current && result !== null) setIssued(null);
    } catch {
      if (active.current)
        setMessage("取消结果不确定；未自动重试。邀请可能仍有效。");
    } finally {
      lock.current = false;
      if (active.current) setWorking(false);
    }
  };

  return (
    <section className={styles.panel} aria-labelledby="invitation-title">
      <h3 id="invitation-title">邀请新成员</h3>
      <p className={styles.note}>
        邀请含一次性密钥，只短时显示。请私下传递；每人使用独立邀请，勿放入日志或共享文件。
      </p>
      {issued ? (
        <p className={styles.status}>
          邀请剩余 {remaining} 秒；隐藏内容不会撤销邀请。
        </p>
      ) : null}
      {invitation ? (
        <label className={styles.form}>
          一次性邀请（仅本次显示）
          <textarea
            className={styles.export}
            readOnly
            rows={4}
            spellCheck={false}
            autoComplete="off"
            value={JSON.stringify(invitation)}
          />
        </label>
      ) : null}
      <div className={styles.actions}>
        <button
          className={styles.button}
          disabled={disabled || working || issued !== null}
          onClick={() => void create()}
        >
          创建一次性邀请
        </button>
        {invitation ? (
          <button className={styles.button} onClick={() => setInvitation(null)}>
            隐藏邀请内容
          </button>
        ) : null}
        {issued ? (
          <button
            className={styles.button + " " + styles.danger}
            disabled={disabled || working}
            onClick={() => void cancel()}
          >
            撤销这份邀请
          </button>
        ) : null}
      </div>
      <p className={styles.note}>
        切走窗口、离开协作页或到期会清空内容。若创建结果不确定，不会自动生成另一份。
      </p>
      {message ? (
        <p role="alert" className={styles.error}>
          {message}
        </p>
      ) : null}
    </section>
  );
}
