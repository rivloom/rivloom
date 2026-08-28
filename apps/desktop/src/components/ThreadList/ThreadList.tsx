import { zhCN } from "../../content/zh-CN";
import type {
  ProjectThreadListAction,
  ProjectThreadsState,
} from "../../hooks/useProjectThreads";
import type { ProjectThread, ProjectThreadStatus } from "../../types/project";

import styles from "./ThreadList.module.css";

type ThreadListProps = {
  state: ProjectThreadsState;
  selectedThreadId: string | null;
  listAction: ProjectThreadListAction | null;
  readingThreadId: string | null;
  onLoadMore: () => void;
  onRefresh: () => void;
  onSelect: (thread: ProjectThread) => void;
};

const maxThreads = 500;
const statusLabels: Record<ProjectThreadStatus, string> = {
  notLoaded: zhCN.thread.status.notLoaded,
  idle: zhCN.thread.status.idle,
  systemError: zhCN.thread.status.systemError,
  active: zhCN.thread.status.active,
};

export function ThreadList({
  state,
  selectedThreadId,
  listAction,
  readingThreadId,
  onLoadMore,
  onRefresh,
  onSelect,
}: ThreadListProps) {
  const threads = threadsOf(state);
  const nextCursor = cursorOf(state);
  const showLoadMore = nextCursor !== null && threads.length < maxThreads;

  return (
    <section className={styles.card} aria-labelledby="thread-list-title">
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>{zhCN.thread.eyebrow}</p>
          <h2 id="thread-list-title" className={styles.title}>
            {zhCN.thread.title}
          </h2>
          <p className={styles.description}>{zhCN.thread.description}</p>
        </div>
        <span className={styles.count}>
          {zhCN.thread.count(threads.length)}
        </span>
      </header>

      <div className={styles.body}>
        {state.state === "loading" ? (
          <div className={styles.placeholder} role="status">
            <span aria-hidden="true">•••</span>
            <div>
              <strong>{zhCN.thread.loading.title}</strong>
              <p>{zhCN.thread.loading.description}</p>
            </div>
          </div>
        ) : null}

        {state.state === "error" ? (
          <div className={styles.errorPanel} role="alert">
            <p>{state.message}</p>
            <button
              type="button"
              disabled={listAction !== null}
              aria-busy={listAction === "refresh"}
              onClick={onRefresh}
            >
              {zhCN.thread.actions.retry}
            </button>
          </div>
        ) : null}

        {state.state === "empty" ? (
          <div className={styles.placeholder}>
            <span aria-hidden="true">＋</span>
            <div>
              <strong>{zhCN.thread.empty.title}</strong>
              <p>{zhCN.thread.empty.description}</p>
            </div>
          </div>
        ) : null}

        {threads.length > 0 ? (
          <ul className={styles.list}>
            {threads.map((thread) => {
              const title = titleOf(thread);
              const selected = thread.id === selectedThreadId;
              const reading = thread.id === readingThreadId;
              const statusLabel = statusLabels[thread.status];
              const timestamp = thread.recencyAt ?? thread.updatedAt;
              return (
                <li key={thread.id} className={styles.row}>
                  <button
                    className={styles.threadButton}
                    type="button"
                    aria-label={zhCN.thread.actions.read(
                      title,
                      statusLabel,
                      selected,
                    )}
                    aria-current={selected ? "true" : undefined}
                    aria-busy={reading}
                    disabled={readingThreadId !== null}
                    onClick={() => onSelect(thread)}
                  >
                    <span className={styles.threadMark} aria-hidden="true">
                      {selected ? "●" : "○"}
                    </span>
                    <span className={styles.copy}>
                      <span className={styles.titleLine}>
                        <strong>{title}</strong>
                        {selected ? (
                          <span className={styles.selectedLabel}>
                            {zhCN.thread.selectedLabel}
                          </span>
                        ) : null}
                        <span
                          className={styles.status}
                          data-status={thread.status}
                        >
                          {statusLabel}
                        </span>
                      </span>
                      {thread.name?.trim() ? (
                        <span className={styles.preview}>{thread.preview}</span>
                      ) : null}
                      <time
                        className={styles.timestamp}
                        dateTime={new Date(timestamp * 1000).toISOString()}
                      >
                        {zhCN.thread.updatedAt(formatTimestamp(timestamp))}
                      </time>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        ) : null}

        {showLoadMore ? (
          <button
            className={styles.loadMore}
            type="button"
            aria-label={zhCN.thread.actions.loadMore}
            disabled={listAction !== null}
            aria-busy={listAction === "loadMore"}
            onClick={onLoadMore}
          >
            {listAction === "loadMore"
              ? zhCN.thread.actions.loadingMore
              : zhCN.thread.actions.loadMore}
          </button>
        ) : null}
      </div>
    </section>
  );
}

function threadsOf(state: ProjectThreadsState) {
  return state.state === "ready" || state.state === "error"
    ? state.threads
    : [];
}

function cursorOf(state: ProjectThreadsState) {
  return state.state === "ready" || state.state === "error"
    ? state.nextCursor
    : null;
}

function titleOf(thread: ProjectThread) {
  return thread.name?.trim() || thread.preview.trim() || zhCN.thread.untitled;
}

function formatTimestamp(timestamp: number) {
  return new Date(timestamp * 1000).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
