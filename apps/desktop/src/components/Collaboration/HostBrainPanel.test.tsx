import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { HostProfile, HostingStatus } from "../../types/collaboration";
import { HostBrainPanel } from "./HostBrainPanel";
vi.mock("../../lib/collaborationInput", () => ({
  certificateFingerprint: async () => "a".repeat(64),
}));
const profile: HostProfile = {
  version: 1,
  binding: {
    brainId: "brain-a",
    memberId: "owner",
    nodeId: "node-a",
    deviceId: "device-a",
  },
  descriptor: {
    version: 1,
    brainId: "brain-a",
    address: "192.168.1.20:7443",
    serverName: "brain.local",
    certificateDer: [1],
  },
  credentialExpiresAt: 2000,
};
function props(status: HostingStatus) {
  return {
    status,
    nodeState: "notConfigured" as const,
    disabled: false,
    onInitialize: vi.fn().mockResolvedValue(null),
    onStart: vi.fn().mockResolvedValue(null),
    onStop: vi.fn().mockResolvedValue(null),
    onOwner: vi.fn().mockResolvedValue(null),
  };
}

it("only initializes an explicit address and does not auto-start or auto-connect", () => {
  const callbacks = props({ state: "notConfigured" });
  const { container } = render(<HostBrainPanel {...callbacks} />);
  expect(callbacks.onInitialize).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("私网监听地址与端口"), {
    target: { value: "192.168.1.20:7443" },
  });
  fireEvent.change(screen.getByLabelText("TLS 服务器名称"), {
    target: { value: "brain.local" },
  });
  fireEvent.submit(container.querySelector("form")!);
  expect(callbacks.onInitialize).toHaveBeenCalledExactlyOnceWith({
    address: "192.168.1.20:7443",
    serverName: "brain.local",
  });
  expect(callbacks.onStart).not.toHaveBeenCalled();
  expect(callbacks.onOwner).not.toHaveBeenCalled();
  expect(
    screen.getByLabelText("私网监听地址与端口").closest("fieldset"),
  ).toMatchSnapshot("explicit private hosting setup");
});

it("exports only public identity and requires exact manual owner confirmation", async () => {
  const callbacks = props({ state: "running", profile });
  render(<HostBrainPanel {...callbacks} />);
  expect(screen.getByLabelText("本机公开 descriptor JSON")).toHaveValue(
    JSON.stringify(profile.descriptor),
  );
  await waitFor(() =>
    expect(screen.getByText("a".repeat(64))).toBeInTheDocument(),
  );
  expect(screen.getByLabelText("确认本机 owner 指纹")).toHaveValue("");
  expect(
    screen.getByRole("button", { name: "以本机 owner 接入" }),
  ).toBeDisabled();
  fireEvent.change(screen.getByLabelText("确认本机 owner 指纹"), {
    target: { value: "a".repeat(64) },
  });
  fireEvent.click(screen.getByRole("button", { name: "以本机 owner 接入" }));
  expect(callbacks.onOwner).toHaveBeenCalledExactlyOnceWith("a".repeat(64));
  expect(screen.getByLabelText("确认本机 owner 指纹")).toHaveValue("");
  fireEvent.click(screen.getByRole("button", { name: "停止监听" }));
  expect(callbacks.onStop).toHaveBeenCalledOnce();
});

it("never offers reinitialization of faulted state or owner enrollment for an existing node", async () => {
  const callbacks = props({ state: "faulted" });
  const { rerender } = render(<HostBrainPanel {...callbacks} />);
  expect(screen.getByRole("alert")).toMatchSnapshot(
    "faulted hosting preserves evidence",
  );
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  rerender(
    <HostBrainPanel
      {...callbacks}
      status={{ state: "stopped", profile }}
      nodeState="disconnected"
    />,
  );
  await screen.findByText("a".repeat(64));
  expect(
    screen.queryByLabelText("确认本机 owner 指纹"),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "启动监听" }));
  expect(callbacks.onStart).toHaveBeenCalledOnce();
});
