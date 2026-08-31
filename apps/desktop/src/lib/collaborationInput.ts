import type {
  BrainInvitation,
  JoinBrainParams,
  TrustDescriptor,
} from "../types/collaboration";

const hex = /^[0-9a-f]{64}$/;
const id = /^[A-Za-z0-9_-]{1,128}$/;

function object(
  text: string,
  maxBytes: number,
  keys: string[],
): Record<string, unknown> {
  if (
    text.length > maxBytes ||
    new TextEncoder().encode(text).length > maxBytes
  )
    throw "invalid";
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw "invalid";
  }
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw "invalid";
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).length !== keys.length ||
    keys.some((key) => !Object.hasOwn(record, key))
  )
    throw "invalid";
  return record;
}

// Preview parsing is not certificate validation or a trust decision. Rust validates the original text.
export function parseDescriptor(text: string): TrustDescriptor {
  const value = object(text, 8192, [
    "version",
    "brainId",
    "address",
    "serverName",
    "certificateDer",
  ]);
  if (
    value.version !== 1 ||
    typeof value.brainId !== "string" ||
    !id.test(value.brainId) ||
    typeof value.address !== "string" ||
    !value.address ||
    value.address.length > 128 ||
    typeof value.serverName !== "string" ||
    !value.serverName ||
    value.serverName.length > 253 ||
    !Array.isArray(value.certificateDer) ||
    value.certificateDer.length === 0 ||
    value.certificateDer.length > 1024 ||
    value.certificateDer.some(
      (byte) => !Number.isInteger(byte) || byte < 0 || byte > 255,
    )
  )
    throw "invalid";
  return value as TrustDescriptor;
}

export async function certificateFingerprint(
  descriptor: TrustDescriptor,
): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new Uint8Array(descriptor.certificateDer),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

export function parseInvitation(
  text: string,
  nowSeconds: number,
): BrainInvitation {
  const value = object(text, 2048, [
    "brainId",
    "invitationId",
    "expiresAt",
    "secret",
  ]);
  if (
    typeof value.brainId !== "string" ||
    !id.test(value.brainId) ||
    typeof value.invitationId !== "string" ||
    !hex.test(value.invitationId) ||
    typeof value.secret !== "string" ||
    !hex.test(value.secret) ||
    typeof value.expiresAt !== "number" ||
    !Number.isSafeInteger(value.expiresAt) ||
    !Number.isSafeInteger(nowSeconds) ||
    value.expiresAt <= nowSeconds ||
    value.expiresAt - nowSeconds > 600
  )
    throw "invalid";
  return value as BrainInvitation;
}

export async function prepareJoin(
  descriptor: string,
  confirmedFingerprint: string,
  invitationText: string,
  nowSeconds: number,
): Promise<JoinBrainParams> {
  const preview = parseDescriptor(descriptor);
  const invitation = parseInvitation(invitationText, nowSeconds);
  if (
    invitation.brainId !== preview.brainId ||
    !hex.test(confirmedFingerprint) ||
    confirmedFingerprint !== (await certificateFingerprint(preview))
  )
    throw "invalid";
  return { descriptor, confirmedFingerprint, invitation };
}
