import { useEffect, useRef, useState } from "react";
import {
  certificateFingerprint,
  parseDescriptor,
  prepareJoin,
} from "../../lib/collaborationInput";
import type {
  JoinBrainParams,
  TrustDescriptor,
} from "../../types/collaboration";
import { TrustSummary } from "./TrustSummary";
import styles from "./Collaboration.module.css";

export function JoinBrainForm({
  disabled,
  onJoin,
}: {
  disabled: boolean;
  onJoin: (params: JoinBrainParams) => Promise<unknown>;
}) {
  const [descriptor, setDescriptor] = useState("");
  const [fingerprint, setFingerprint] = useState("");
  const [invitation, setInvitation] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [preview, setPreview] = useState<{
    descriptor: TrustDescriptor;
    fingerprint: string;
  } | null>(null);
  const [error, setError] = useState(false);
  const [working, setWorking] = useState(false);
  const lock = useRef(false);
  const epoch = useRef(0);
  useEffect(() => {
    const hide = () => {
      if (document.hidden) {
        ++epoch.current;
        setInvitation("");
        setConfirmed(false);
      }
    };
    document.addEventListener("visibilitychange", hide);
    return () => {
      ++epoch.current;
      document.removeEventListener("visibilitychange", hide);
    };
  }, []);

  const inspect = async () => {
    const generation = epoch.current;
    setError(false);
    try {
      const value = parseDescriptor(descriptor);
      const hash = await certificateFingerprint(value);
      if (generation === epoch.current)
        setPreview({ descriptor: value, fingerprint: hash });
    } catch {
      if (generation === epoch.current) {
        setPreview(null);
        setError(true);
      }
    }
  };

  const join = async () => {
    if (disabled || lock.current || !confirmed || document.hidden) return;
    lock.current = true;
    setWorking(true);
    setError(false);
    const generation = epoch.current;
    const transientInvitation = invitation;
    setInvitation("");
    try {
      const params = await prepareJoin(
        descriptor,
        fingerprint,
        transientInvitation,
        Math.floor(Date.now() / 1000),
      );
      if (generation === epoch.current) await onJoin(params);
    } catch {
      if (generation === epoch.current) setError(true);
    } finally {
      lock.current = false;
      // The component may be hidden while preparing input; never restore its invitation.
      setWorking(false);
      setConfirmed(false);
    }
  };

  return (
    <section className={styles.panel} aria-labelledby="join-brain-title">
      <h3 id="join-brain-title">加入已有 Brain</h3>
      <p className={styles.note}>
        先从管理员取得公开描述，再通过另一条可信渠道核对指纹。导入不代表信任。
      </p>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          void join();
        }}
        autoComplete="off"
      >
        <fieldset disabled={disabled || working}>
          <label>
            公开 descriptor JSON
            <textarea
              rows={4}
              maxLength={8192}
              required
              spellCheck={false}
              value={descriptor}
              onChange={(event) => {
                ++epoch.current;
                setDescriptor(event.target.value);
                setPreview(null);
                setConfirmed(false);
              }}
            />
          </label>
          <div className={styles.actions}>
            <button
              className={styles.button}
              type="button"
              onClick={() => void inspect()}
            >
              预览公开身份
            </button>
          </div>
          {preview ? (
            <div>
              <p className={styles.notice}>
                以下指纹来自导入内容，仅供核对；不能作为独立确认来源。
              </p>
              <TrustSummary {...preview} />
            </div>
          ) : null}
          <label>
            独立渠道取得的完整指纹
            <input
              value={fingerprint}
              onChange={(event) => {
                setFingerprint(event.target.value);
                setConfirmed(false);
              }}
              maxLength={64}
              pattern="[0-9a-f]{64}"
              required
              spellCheck={false}
              placeholder="64 位小写十六进制 SHA-256"
            />
          </label>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
              required
            />
            我已通过独立可信渠道核对 Brain、地址与指纹
          </label>
          <label>
            一次性邀请 JSON
            <textarea
              value={invitation}
              onChange={(event) => setInvitation(event.target.value)}
              rows={3}
              maxLength={2048}
              spellCheck={false}
              required
            />
          </label>
          <p className={styles.note}>
            邀请含临时密钥；提交或窗口隐藏时清空。失败不会自动重试，可能需要管理员协助恢复。
          </p>
          <button
            className={styles.button + " " + styles.primary}
            type="submit"
            disabled={!confirmed || working}
          >
            {working ? "正在加入…" : "确认信任并加入"}
          </button>
        </fieldset>
        {error ? (
          <p role="alert" className={styles.error}>
            无法加入：请检查公开描述、独立指纹及邀请的 Brain
            和有效期。未自动重试。
          </p>
        ) : null}
      </form>
    </section>
  );
}
