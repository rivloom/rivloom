import { useState } from "react";

import { zhCN } from "../../content/zh-CN";
import { useProjectThreads } from "../../hooks/useProjectThreads";
import type { LocalProject, ProjectThread } from "../../types/project";
import { ThreadList } from "../ThreadList/ThreadList";

import styles from "./ProjectWorkspace.module.css";

type ProjectWorkspaceProps = {
  project: LocalProject;
  runtimeConnected: boolean;
  onBack: () => void;
};

export function ProjectWorkspace({
  project,
  runtimeConnected,
  onBack,
}: ProjectWorkspaceProps) {
  const threads = useProjectThreads(project.id, runtimeConnected);
  const [selection, setSelection] = useState<{
    projectId: string;
    thread: ProjectThread;
  } | null>(null);
  const selectedThread =
    selection?.projectId === project.id ? selection.thread : null;
  const starting = threads.threadAction?.type === "start";
  const readingThreadId =
    threads.threadAction?.type === "read"
      ? threads.threadAction.threadId
      : null;

  const handleStart = async () => {
    const thread = await threads.startThread();
    if (thread) setSelection({ projectId: project.id, thread });
  };

  const handleSelect = async (thread: ProjectThread) => {
    const read = await threads.readThread(thread.id);
    if (read) setSelection({ projectId: project.id, thread: read });
  };

  return (
    <section
      className={styles.workspace}
      aria-label={zhCN.workspace.label(project.name)}
    >
      <header className={styles.header}>
        <button className={styles.backButton} type="button" onClick={onBack}>
          <span aria-hidden="true">←</span>
          {zhCN.workspace.actions.back}
        </button>
        <div className={styles.heading}>
          <p>{zhCN.workspace.eyebrow}</p>
          <h2 id="project-workspace-title">{project.name}</h2>
          <span>
            <strong>{zhCN.workspace.pathLabel}</strong>
            <code>{project.path}</code>
          </span>
        </div>
        <button
          className={styles.primaryButton}
          type="button"
          disabled={
            !runtimeConnected ||
            threads.state.state === "loading" ||
            threads.threadAction !== null
          }
          aria-busy={starting}
          onClick={() => void handleStart()}
        >
          <span aria-hidden="true">＋</span>
          {starting
            ? zhCN.workspace.actions.starting
            : zhCN.workspace.actions.start}
        </button>
      </header>

      {!runtimeConnected ? (
        <div className={styles.disconnected} role="alert">
          <span aria-hidden="true">!</span>
          <div>
            <strong>{zhCN.workspace.disconnected.title}</strong>
            <p>{zhCN.workspace.disconnected.description}</p>
          </div>
        </div>
      ) : (
        <div className={styles.content}>
          {threads.actionError ? (
            <p className={styles.actionError} role="alert">
              {threads.actionError}
            </p>
          ) : null}

          <ThreadList
            state={threads.state}
            selectedThreadId={selectedThread?.id ?? null}
            listAction={threads.listAction}
            readingThreadId={readingThreadId}
            onLoadMore={() => void threads.loadMore()}
            onRefresh={() => void threads.refresh()}
            onSelect={(thread) => void handleSelect(thread)}
          />

          {selectedThread ? (
            <section
              className={styles.selection}
              aria-labelledby="selected-thread-title"
            >
              <p className={styles.selectionEyebrow}>
                {zhCN.workspace.selection.eyebrow}
              </p>
              <h3 id="selected-thread-title">{titleOf(selectedThread)}</h3>
              {selectedThread.name?.trim() ? (
                <p className={styles.preview}>{selectedThread.preview}</p>
              ) : null}
              <div className={styles.placeholder}>
                <span aria-hidden="true">↗</span>
                <div>
                  <strong>{zhCN.workspace.selection.placeholderTitle}</strong>
                  <p>{zhCN.workspace.selection.placeholderDescription}</p>
                </div>
              </div>
            </section>
          ) : null}
        </div>
      )}
    </section>
  );
}

function titleOf(thread: ProjectThread) {
  return thread.name?.trim() || thread.preview.trim() || zhCN.thread.untitled;
}
