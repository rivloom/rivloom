import userEvent from "@testing-library/user-event";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { IdentityState } from "../../hooks/useIdentity";
import type { RivloomIdentity } from "../../types/identity";
import { IdentityCard } from "./IdentityCard";

const identity: RivloomIdentity = {
  identityId: "identity-v1-11111111111111111111111111111111",
  displayName: "本机用户",
  deviceId: "device-v1-22222222222222222222222222222222",
  brainMembership: null,
};

function renderIdentity(state: IdentityState = { state: "ready", identity }) {
  const onRefresh = vi.fn();
  const view = render(
    <IdentityCard state={state} pendingAction={null} onRefresh={onRefresh} />,
  );
  return { onRefresh, ...view };
}

describe("IdentityCard", () => {
  it("shows a local Rivloom identity without claiming Brain membership", () => {
    renderIdentity();

    expect(
      screen.getByRole("heading", { name: "Rivloom 身份" }),
    ).toBeInTheDocument();
    expect(screen.getByText("本机用户")).toBeInTheDocument();
    expect(screen.getByText("连接状态见下方协作区")).toBeInTheDocument();
    expect(
      screen.getByText("连接状态见下方协作区").closest("dl"),
    ).toMatchSnapshot("local identity details without Brain");
  });

  it("offers a retry when identity storage is unavailable", async () => {
    const user = userEvent.setup();
    const { onRefresh } = renderIdentity({
      state: "error",
      message: "Rivloom 身份暂时不可用。",
    });

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Rivloom 身份暂时不可用。",
    );
    await user.click(screen.getByRole("button", { name: "重新读取身份" }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });
});
