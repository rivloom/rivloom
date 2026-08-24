import userEvent from "@testing-library/user-event";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ServiceStatusCard } from "./ServiceStatusCard";

describe("ServiceStatusCard", () => {
  it("announces the starting state with text", () => {
    render(<ServiceStatusCard status={{ state: "starting" }} />);

    expect(screen.getByText("正在启动")).toBeInTheDocument();
    expect(screen.getByText("正在准备本地核心服务…")).toBeInTheDocument();
  });

  it("shows connected runtime details", () => {
    render(
      <ServiceStatusCard
        status={{
          state: "connected",
          appVersion: "0.1.0-alpha.0",
          appServerUserAgent: "codex-app-server/1.2.3",
          platform: "Windows 11 · x86_64",
          codexHome: "C:\\Users\\demo\\AppData\\Roaming\\Rivloom\\codex-home",
        }}
      />,
    );

    expect(screen.getByText("已连接")).toBeInTheDocument();
    expect(screen.getByText("codex-app-server/1.2.3")).toBeInTheDocument();
    expect(screen.getByText("Windows 11 · x86_64")).toBeInTheDocument();
    expect(screen.getByText("0.1.0-alpha.0")).toBeInTheDocument();
  });

  it("announces errors and lets the keyboard retry", async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();

    render(
      <ServiceStatusCard
        status={{
          state: "error",
          message: "核心服务暂时无法启动。",
          retryable: true,
        }}
        onRetry={onRetry}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "核心服务暂时无法启动。",
    );

    const retryButton = screen.getByRole("button", { name: "重试连接" });
    expect(retryButton).toBeEnabled();

    retryButton.focus();
    await user.keyboard("{Enter}");

    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("disables retry when the error is not retryable", () => {
    render(
      <ServiceStatusCard
        status={{
          state: "error",
          message: "核心服务文件缺失。",
          retryable: false,
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "重试连接" })).toBeDisabled();
  });
});
