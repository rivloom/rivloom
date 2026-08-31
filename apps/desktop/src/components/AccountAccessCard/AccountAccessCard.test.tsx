import userEvent from "@testing-library/user-event";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AccountAction } from "../../hooks/useAccountStatus";
import type { CodexRuntimeAuthStatus } from "../../types/account";
import { AccountAccessCard } from "./AccountAccessCard";

const defaultStatus: CodexRuntimeAuthStatus = { state: "signedOut" };

function renderAccount(
  options: {
    runtimeConnected?: boolean;
    status?: CodexRuntimeAuthStatus;
    pendingAction?: AccountAction | null;
  } = {},
) {
  const callbacks = {
    onRefresh: vi.fn(),
    onStartChatgptLogin: vi.fn(),
    onCancelLogin: vi.fn(),
    onLogout: vi.fn(),
  };

  render(
    <AccountAccessCard
      runtimeConnected={options.runtimeConnected ?? true}
      status={options.status ?? defaultStatus}
      pendingAction={options.pendingAction ?? null}
      {...callbacks}
    />,
  );

  return callbacks;
}

describe("AccountAccessCard", () => {
  it("disables account access until the core runtime is connected", () => {
    renderAccount({
      runtimeConnected: false,
      status: { state: "signedIn", email: null, planType: "plus" },
    });

    expect(screen.getByText("等待核心服务")).toBeInTheDocument();
    expect(screen.getByText("核心服务连接后可登录")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("announces the checking state", () => {
    renderAccount({ status: { state: "checking" } });

    expect(screen.getByText("正在检查")).toBeInTheDocument();
    expect(screen.getByText("正在读取账号状态…")).toBeInTheDocument();
  });

  it("offers only browser login and disables it while the action is pending", async () => {
    const user = userEvent.setup();
    const callbacks = renderAccount();

    await user.click(screen.getByRole("button", { name: "使用浏览器登录" }));

    expect(callbacks.onStartChatgptLogin).toHaveBeenCalledOnce();
    expect(screen.getAllByRole("button")).toHaveLength(1);

    cleanup();
    renderAccount({ pendingAction: "startChatgptLogin" });
    expect(
      screen.getByRole("button", { name: "使用浏览器登录" }),
    ).toBeDisabled();
  });

  it("allows cancelling while browser login is pending", async () => {
    const user = userEvent.setup();
    const callbacks = renderAccount({ status: { state: "browserPending" } });

    expect(screen.getByText("请在浏览器完成登录")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消登录" }));

    expect(callbacks.onCancelLogin).toHaveBeenCalledOnce();
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("names the logout dialog, focuses cancel, handles Escape, and restores focus", async () => {
    const user = userEvent.setup();
    const callbacks = renderAccount({
      status: {
        state: "signedIn",
        email: "user@example.com",
        planType: "plus",
      },
    });
    const logoutButton = screen.getByRole("button", { name: "退出账号" });

    await user.click(logoutButton);
    expect(
      screen.getByRole("dialog", { name: "退出 ChatGPT 账号？" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "暂不退出" })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(logoutButton).toHaveFocus();

    await user.click(logoutButton);
    await user.click(screen.getByRole("button", { name: "确认退出" }));

    expect(callbacks.onLogout).toHaveBeenCalledOnce();
    await waitFor(() => expect(logoutButton).toHaveFocus());
  });

  it("announces safe errors and only enables retry when allowed", async () => {
    const user = userEvent.setup();
    const callbacks = renderAccount({
      status: {
        state: "error",
        message: "账号状态暂时不可用。",
        retryable: true,
      },
    });

    expect(screen.getByRole("alert")).toHaveTextContent("账号状态暂时不可用。");
    await user.click(screen.getByRole("button", { name: "重新检查" }));
    expect(callbacks.onRefresh).toHaveBeenCalledOnce();
  });
});
