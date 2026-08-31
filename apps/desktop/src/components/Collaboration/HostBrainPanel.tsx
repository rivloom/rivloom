import { useEffect, useState } from "react";
import { certificateFingerprint } from "../../lib/collaborationInput";
import type { HostingStatus, NodeStatus } from "../../types/collaboration";
import { TrustSummary } from "./TrustSummary";
import styles from "./Collaboration.module.css";

type Props = {
  status: HostingStatus;
  nodeState: NodeStatus["state"];
  disabled: boolean;
  onInitialize: (params: {
    address: string;
    serverName: string;
  }) => Promise<unknown>;
  onStart: () => Promise<unknown>;
  onStop: () => Promise<unknown>;
  onOwner: (fingerprint: string) => Promise<unknown>;
};

export function HostBrainPanel({
  status,
  nodeState,
  disabled,
  onInitialize,
  onStart,
  onStop,
  onOwner,
}: Props) {
  const [address, setAddress] = useState("");
  const [serverName, setServerName] = useState("");
  const [fingerprint, setFingerprint] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [hashError, setHashError] = useState(false);
  const profile = "profile" in status ? status.profile : null;
  useEffect(() => {
    let active = true;
    setFingerprint("");
    setHashError(false);
    if (profile)
      void certificateFingerprint(profile.descriptor)
        .then((hash) => {
          if (active) setFingerprint(hash);
        })
        .catch(() => {
          if (active) setHashError(true);
        });
    return () => {
      active = false;
    };
  }, [profile]);

  return (
    <section className={styles.panel} aria-labelledby="host-brain-title">
      <h3 id="host-brain-title">在本机托管 Brain</h3>
      <p className={styles.note}>
        一台设备维护成员与协作状态。托管和 Codex Runtime 登录互不依赖。
      </p>
      {status.state === "notConfigured" ? (
        <form
          className={styles.form}
          onSubmit={(event) => {
            event.preventDefault();
            if (!disabled)
              void onInitialize({
                address: address.trim(),
                serverName: serverName.trim(),
              });
          }}
        >
          <fieldset disabled={disabled}>
            <label>
              私网监听地址与端口
              <input
                value={address}
                onChange={(event) => setAddress(event.target.value)}
                placeholder="例如 192.168.1.20:7443"
                maxLength={128}
                required
              />
            </label>
            <label>
              TLS 服务器名称
              <input
                value={serverName}
                onChange={(event) => setServerName(event.target.value)}
                placeholder="例如 brain.local"
                maxLength={253}
                required
              />
            </label>
            <p className={styles.notice}>
              使用本机实际拥有的 LAN / Tailscale 私网地址。127.0.0.1
              只能本机连接，不能用于两设备验收。初始化会保存身份和 OS
              凭证，不会自动开始监听。
            </p>
            <button type="submit" className={styles.button}>
              初始化本机 Brain
            </button>
          </fieldset>
        </form>
      ) : null}
      {status.state === "faulted" ? (
        <p role="alert" className={styles.error}>
          本机 Brain
          登记不完整或不可读取。已停止操作；保留现场，不能覆盖初始化。
        </p>
      ) : null}
      {profile ? (
        <>
          <p className={styles.status}>
            {status.state === "running"
              ? "本机 Brain 正在监听"
              : "本机 Brain 已停止监听"}
          </p>
          <TrustSummary
            descriptor={profile.descriptor}
            fingerprint={fingerprint || "正在计算…"}
          />
          {hashError ? (
            <p role="alert" className={styles.error}>
              无法计算本机证书指纹；暂不能确认 owner 接入。
            </p>
          ) : null}
          <details>
            <summary>查看公开连接描述</summary>
            <p className={styles.note}>
              将描述交给成员；请通过另一条可信渠道确认完整指纹。此处不含邀请或私钥。
            </p>
            <textarea
              className={styles.export}
              aria-label="本机公开 descriptor JSON"
              readOnly
              rows={4}
              value={JSON.stringify(profile.descriptor)}
              spellCheck={false}
            />
          </details>
          <p className={styles.note}>
            停止监听会中断客户端连接；下次启动仍使用同一 Brain
            登记。连接与重连均需明确操作。
          </p>
          <div className={styles.actions}>
            {status.state === "running" ? (
              <button
                className={styles.button}
                disabled={disabled}
                onClick={() => void onStop()}
              >
                停止监听
              </button>
            ) : (
              <button
                className={styles.button + " " + styles.primary}
                disabled={disabled}
                onClick={() => void onStart()}
              >
                启动监听
              </button>
            )}
          </div>
          {nodeState === "notConfigured" && status.state === "running" ? (
            <form
              className={styles.form}
              onSubmit={(event) => {
                event.preventDefault();
                if (!disabled && fingerprint && confirmation === fingerprint) {
                  setConfirmation("");
                  void onOwner(confirmation);
                }
              }}
            >
              <fieldset disabled={disabled}>
                <label>
                  确认本机 owner 指纹
                  <input
                    value={confirmation}
                    onChange={(event) => setConfirmation(event.target.value)}
                    maxLength={64}
                    pattern="[0-9a-f]{64}"
                    required
                    spellCheck={false}
                    autoComplete="off"
                  />
                </label>
                <p className={styles.note}>
                  核对上方本机 Brain 身份后，输入完整指纹。仅恢复本机已有 owner
                  凭证，不兑换邀请。
                </p>
                <button
                  className={styles.button}
                  type="submit"
                  disabled={!fingerprint || confirmation !== fingerprint}
                >
                  以本机 owner 接入
                </button>
              </fieldset>
            </form>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
