import userEvent from "@testing-library/user-event";
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { RecentProjectAction } from "../../hooks/useRecentProjects";
import type {
  LocalProject,
  PersistenceWarning,
  ProjectSelection,
} from "../../types/project";
import { ProjectAccessCard } from "./ProjectAccessCard";

const availableProject: LocalProject = {
  id: "project-available",
  path: "C:\\workspaces\\a-very-long-folder-name\\rivloom-demo",
  name: "Rivloom Demo",
  lastOpenedAt: 1_787_827_600,
  availability: "available",
};
const missingProject: LocalProject = {
  ...availableProject,
  id: "project-missing",
  name: "Missing Demo",
  availability: "missing",
};
const unreadableProject: LocalProject = {
  ...availableProject,
  id: "project-unreadable",
  name: "Private Demo",
  availability: "unreadable",
};

type RenderOptions = {
  state?: Parameters<typeof ProjectAccessCard>[0]["state"];
  pendingAction?: RecentProjectAction | null;
  warning?: PersistenceWarning | null;
  activeProjectId?: string | null;
  selection?: ProjectSelection | null;
};

function renderProjectCard(options: RenderOptions = {}) {
  const callbacks = {
    onRefresh: vi.fn(),
    onSelect: vi.fn().mockResolvedValue(options.selection ?? null),
    onOpenProject: vi.fn(),
    onRemove: vi.fn(),
  };
  render(
    <ProjectAccessCard
      state={options.state ?? { state: "empty" }}
      pendingAction={options.pendingAction ?? null}
      warning={options.warning ?? null}
      activeProjectId={options.activeProjectId ?? null}
      {...callbacks}
    />,
  );
  return callbacks;
}

function cardSnapshot() {
  const card = screen.getByRole("region", { name: "本地项目" });
  const text = card.cloneNode(true) as HTMLElement;
  text.querySelectorAll("time").forEach((time) => {
    time.textContent = "<time>";
  });
  return [
    text.textContent?.replace(/\s+/g, " ").trim(),
    ...within(card)
      .queryAllByRole("button")
      .map((button) =>
        [
          button.getAttribute("aria-label") ?? button.textContent,
          (button as HTMLButtonElement).disabled ? "disabled" : "enabled",
          button.getAttribute("aria-busy") === "true" ? "busy" : null,
        ]
          .filter(Boolean)
          .join(" | "),
      ),
  ];
}

describe("ProjectAccessCard", () => {
  it("keeps dialog cancellation quiet in the empty state", async () => {
    const user = userEvent.setup();
    const callbacks = renderProjectCard();

    expect(screen.getByText("还没有最近项目")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开本地项目" }));

    expect(callbacks.onSelect).toHaveBeenCalledOnce();
    expect(callbacks.onOpenProject).not.toHaveBeenCalled();
    expect(cardSnapshot()).toMatchSnapshot("empty recent projects");
  });

  it("opens selected and recent available projects", async () => {
    const user = userEvent.setup();
    const selection = { project: availableProject, warning: null } as const;
    const callbacks = renderProjectCard({
      state: { state: "ready", projects: [availableProject] },
      activeProjectId: availableProject.id,
      selection,
    });

    await user.click(screen.getByRole("button", { name: "打开本地项目" }));
    await user.click(
      screen.getByRole("button", {
        name: "打开项目 Rivloom Demo，当前项目",
      }),
    );

    expect(callbacks.onOpenProject.mock.calls).toEqual([
      [availableProject],
      [availableProject],
    ]);
    expect(
      screen.getByRole("button", {
        name: "打开项目 Rivloom Demo，当前项目",
      }),
    ).toHaveAttribute("aria-current", "page");
    expect(screen.getByText("当前项目")).toBeInTheDocument();
    expect(screen.getByText(availableProject.path)).toBeInTheDocument();
    expect(cardSnapshot()).toMatchSnapshot("populated recent projects");
  });

  it("disables unavailable projects but keeps remove accessible", async () => {
    const user = userEvent.setup();
    const callbacks = renderProjectCard({
      state: {
        state: "ready",
        projects: [missingProject, unreadableProject],
      },
    });

    expect(
      screen.getByRole("button", {
        name: "打开项目 Missing Demo，目录已不存在",
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", {
        name: "打开项目 Private Demo，目录无法访问",
      }),
    ).toBeDisabled();
    expect(screen.getByText("目录已不存在")).toBeInTheDocument();
    expect(screen.getByText("目录无法访问")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "从最近项目移除 Missing Demo" }),
    );
    expect(callbacks.onRemove).toHaveBeenCalledWith(missingProject.id);
    expect(cardSnapshot()).toMatchSnapshot("unavailable recent projects");
  });

  it("announces recoverable list and storage failures", async () => {
    const user = userEvent.setup();
    const callbacks = renderProjectCard({
      state: {
        state: "error",
        message: "最近项目暂时不可用。",
        projects: [],
      },
      warning: "recentProjectsNotSaved",
    });

    expect(screen.getByRole("alert")).toHaveTextContent("最近项目暂时不可用。");
    expect(screen.getByRole("status")).toHaveTextContent(
      "项目已打开，但最近项目未能保存。",
    );
    expect(screen.queryByText("还没有最近项目")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重新加载" }));
    expect(callbacks.onRefresh).toHaveBeenCalledOnce();
  });

  it("labels keyboard actions and blocks them while another action runs", async () => {
    const user = userEvent.setup();
    renderProjectCard({
      state: { state: "ready", projects: [availableProject] },
    });
    const selectButton = screen.getByRole("button", {
      name: "打开本地项目",
    });
    const openButton = screen.getByRole("button", {
      name: "打开项目 Rivloom Demo",
    });
    const removeButton = screen.getByRole("button", {
      name: "从最近项目移除 Rivloom Demo",
    });

    await user.tab();
    expect(selectButton).toHaveFocus();
    await user.tab();
    expect(openButton).toHaveFocus();
    await user.tab();
    expect(removeButton).toHaveFocus();

    renderProjectCard({
      state: { state: "ready", projects: [availableProject] },
      pendingAction: { type: "select" },
    });
    expect(
      screen.getAllByRole("button", { name: "打开本地项目" })[1],
    ).toHaveAttribute("aria-busy", "true");
    expect(
      screen.getAllByRole("button", { name: "打开项目 Rivloom Demo" })[1],
    ).toBeDisabled();
  });
});
