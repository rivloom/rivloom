// @vitest-environment node
import { expect, it } from "vitest";
import {
  certificateFingerprint,
  parseDescriptor,
  parseInvitation,
  prepareJoin,
} from "./collaborationInput";

const descriptor = {
  version: 1 as const,
  brainId: "brain-a",
  address: "127.0.0.1:7443",
  serverName: "localhost",
  certificateDer: [97, 98, 99],
};
const fingerprint =
  "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const invitation = {
  brainId: "brain-a",
  invitationId: "a".repeat(64),
  secret: "b".repeat(64),
  expiresAt: 1500,
};

it("checks independent fingerprint and Brain binding, preserving original descriptor for Rust validation", async () => {
  const text = JSON.stringify(descriptor, null, 2);
  expect(await certificateFingerprint(parseDescriptor(text))).toBe(fingerprint);
  await expect(
    prepareJoin(text, fingerprint, JSON.stringify(invitation), 1000),
  ).resolves.toEqual({
    descriptor: text,
    confirmedFingerprint: fingerprint,
    invitation,
  });
  for (const confirmation of ["", "a".repeat(64), fingerprint.toUpperCase()]) {
    await expect(
      prepareJoin(text, confirmation, JSON.stringify(invitation), 1000),
    ).rejects.toBe("invalid");
  }
  await expect(
    prepareJoin(
      text,
      fingerprint,
      JSON.stringify({ ...invitation, brainId: "other" }),
      1000,
    ),
  ).rejects.toBe("invalid");
  await expect(
    prepareJoin(
      JSON.stringify({ ...descriptor, certificateDer: [100] }),
      fingerprint,
      JSON.stringify(invitation),
      1000,
    ),
  ).rejects.toBe("invalid");
});

it("bounds untrusted input and never includes rejected text in errors", () => {
  for (const value of [
    "private input",
    " ".repeat(8193),
    JSON.stringify({ ...descriptor, extra: true }),
    JSON.stringify({ ...descriptor, brainId: "brain:wrong" }),
    JSON.stringify({ ...descriptor, certificateDer: [256] }),
    JSON.stringify({ ...descriptor, certificateDer: Array(1025).fill(0) }),
    JSON.stringify(descriptor) + " ".repeat(8192),
    '"' + "密".repeat(3000) + '"',
  ])
    expect(() => parseDescriptor(value)).toThrow("invalid");
});

it("rejects expired, overlong, malformed and unknown-field invitation envelopes", () => {
  expect(parseInvitation(JSON.stringify(invitation), 1000)).toEqual(invitation);
  for (const value of [
    { ...invitation, expiresAt: 1000 },
    { ...invitation, expiresAt: 1601 },
    { ...invitation, expiresAt: 1000.5 },
    { ...invitation, secret: "short" },
    { ...invitation, invitationId: "A".repeat(64) },
    { ...invitation, extra: true },
  ])
    expect(() => parseInvitation(JSON.stringify(value), 1000)).toThrow(
      "invalid",
    );
  expect(() => parseInvitation(" ".repeat(2049), 1000)).toThrow("invalid");
});
