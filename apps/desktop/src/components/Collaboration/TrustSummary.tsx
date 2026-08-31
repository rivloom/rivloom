import type { TrustDescriptor } from "../../types/collaboration";
import styles from "./Collaboration.module.css";

export function TrustSummary({
  descriptor,
  fingerprint,
}: {
  descriptor: TrustDescriptor;
  fingerprint: string;
}) {
  return (
    <dl className={styles.details}>
      <div>
        <dt>Brain ID</dt>
        <dd>
          <code>{descriptor.brainId}</code>
        </dd>
      </div>
      <div>
        <dt>连接地址 / TLS 名称</dt>
        <dd>
          <code>{descriptor.address + " / " + descriptor.serverName}</code>
        </dd>
      </div>
      <div>
        <dt>证书 SHA-256 指纹</dt>
        <dd>
          <code>{fingerprint}</code>
        </dd>
      </div>
    </dl>
  );
}
