import userEvent from "@testing-library/user-event";
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ProjectThreadsState } from "../../hooks/useProjectThreads";
import type { ProjectThread } from "../../types/project";
import { ThreadList } from "./ThreadList";

const activeThread: ProjectThread = {
  id: "thr-active",
  name: "修复登录回归",
  preview:
    "检查一个很长的会话摘要是否能在窄窗口中自然换行，而不会把操作按钮推出卡片边界。",
  createdAt: 1_787_827_000,
  updatedAt: 1_787_827_300,
  recencyAt: 1_787_827_600,
  status: "active",
  cwd: "C:\\workspaces\\rivloom-demo",
};
const unnamedThread: ProjectThread = {
  ...activeThread,
  id: "thr-unnamed",
  name: null,
  preview: "整理设置页",
  updatedAt: 1_787_820_000,
  recencyAt: null,
  status: "idle",
};

type RenderOptions = {
  state?: ProjectThreadsState;
  selectedThreadId?: string | null;
  listAction?: "refresh" | "loadMore" | null;
  readingThreadId?: string | null;
};

function renderThreadList(options: RenderOptions = {}) {
  const callbacks = {
    onLoadMore: vi.fn(),
    onRefresh: vi.fn(),
    onSelect: vi.fn(),
  };
  const rendered = render(
    <ThreadList
      state={options.state ?? { state: "empty" }}
      selectedThreadId={options.selectedThreadId ?? null}
      listAction={options.listAction ?? null}
      readingThreadId={options.readingThreadId ?? null}
      {...callbacks}
    />,
  );
  return { ...callbacks, ...rendered };
}

function listSnapshot() {
  const region = screen.getByRole("region", { name: "项目会话" });
  const copy = region.cloneNode(true) as HTMLElement;
  copy.querySelectorAll("time").forEach((time) => {
    time.textContent = "<time>";
  });
  return [
    copy.textContent?.replace(/\s+/g, " ").trim(),
    ...within(region)
      .queryAllByRole("button")
      .map((button) =>
        [
          button.getAttribute("aria-label") ?? button.textContent,
          (button as HTMLButtonElement).disabled ? "disabled" : "enabled",
          button.getAttribute("aria-current"),
          button.getAttribute("aria-busy") === "true" ? "busy" : null,
        ]
          .filter(Boolean)
          .join(" | "),
      ),
  ];
}

describe("ThreadList", () => {
  it("renders loading and empty states", () => {
    const { unmount } = render(
      <ThreadList
        state={{ state: "loading" }}
        selectedThreadId={null}
        listAction={null}
        readingThreadId={null}
        onLoadMore={vi.fn()}
        onRefresh={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("正在读取项目会话");
    unmount();

    renderThreadList();
    expect(screen.getByText("还没有项目会话")).toBeInTheDocument();
    expect(listSnapshot()).toMatchSnapshot("empty thread list");
  });

  it("keeps loaded rows visible when retrying a list failure", async () => {
    const user = userEvent.setup();
    const callbacks = renderThreadList({
      state: {
        state: "error",
        message: "会话列表暂时不可用。",
        threads: [activeThread],
        nextCursor: "retry-cursor",
      },
    });

    expect(screen.getByRole("alert")).toHaveTextContent("会话列表暂时不可用。");
    expect(screen.getByText(activeThread.name!)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重新加载会话" }));
    expect(callbacks.onRefresh).toHaveBeenCalledOnce();
    expect(listSnapshot()).toMatchSnapshot("thread list failure");
  });

  it("supports keyboard selection, status labels, and timestamp fallback", async () => {
    const user = userEvent.setup();
    const callbacks = renderThreadList({
      state: {
        state: "ready",
        threads: [activeThread, unnamedThread],
        nextCursor: "next-page",
      },
      selectedThreadId: activeThread.id,
    });

    const activeButton = screen.getByRole("button", {
      name: "查看会话 修复登录回归，状态进行中，当前会话",
    });
    expect(activeButton).toHaveAttribute("aria-current", "true");
    expect(screen.getByText("进行中")).toBeInTheDocument();
    expect(screen.getByText("可继续")).toBeInTheDocument();
    expect(screen.getAllByRole("time")[1]).toHaveAttribute(
      "datetime",
      new Date(unnamedThread.updatedAt * 1000).toISOString(),
    );

    await user.tab();
    expect(activeButton).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(callbacks.onSelect).toHaveBeenCalledWith(activeThread);
    await user.tab();
    expect(
      screen.getByRole("button", {
        name: "查看会话 整理设置页，状态可继续",
      }),
    ).toHaveFocus();
    expect(listSnapshot()).toMatchSnapshot("populated thread list");
  });

  it("loads more only below the 500-thread bound", async () => {
    const user = userEvent.setup();
    const pending = renderThreadList({
      state: {
        state: "ready",
        threads: [activeThread, unnamedThread],
        nextCursor: "next-page",
      },
      listAction: "loadMore",
      readingThreadId: activeThread.id,
    });
    const loadMore = screen.getByRole("button", { name: "加载更多会话" });
    expect(loadMore).toBeDisabled();
    expect(loadMore).toHaveAttribute("aria-busy", "true");
    expect(
      screen.getByRole("button", {
        name: "查看会话 修复登录回归，状态进行中",
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: "查看会话 整理设置页，状态可继续",
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: "查看会话 修复登录回归，状态进行中",
      }),
    ).toHaveAttribute("aria-busy", "true");
    expect(
      screen.getByRole("button", {
        name: "查看会话 整理设置页，状态可继续",
      }),
    ).toHaveAttribute("aria-busy", "false");

    pending.unmount();
    const available = renderThreadList({
      state: {
        state: "ready",
        threads: [activeThread],
        nextCursor: "next-page",
      },
    });
    await user.click(screen.getByRole("button", { name: "加载更多会话" }));
    expect(available.onLoadMore).toHaveBeenCalledOnce();
    available.unmount();

    const belowBound = renderThreadList({
      state: {
        state: "ready",
        threads: Array.from({ length: 499 }, (_, index) => ({
          ...activeThread,
          id: `thread-${index}`,
        })),
        nextCursor: "last-page",
      },
    });
    await user.click(screen.getByRole("button", { name: "加载更多会话" }));
    expect(belowBound.onLoadMore).toHaveBeenCalledOnce();
    belowBound.unmount();

    renderThreadList({
      state: {
        state: "ready",
        threads: Array.from({ length: 500 }, (_, index) => ({
          ...activeThread,
          id: `thread-${index}`,
        })),
        nextCursor: "must-not-be-used",
      },
    });
    expect(
      screen.queryByRole("button", { name: "加载更多会话" }),
    ).not.toBeInTheDocument();
  });
});
