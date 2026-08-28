# Rivloom Desktop A2 Local Projects and Threads Implementation Plan

> **For Codex/Claude:** REQUIRED SKILL: Use `executing-plans` to implement this plan task-by-task in a dedicated worktree.

**Goal:** Add safe local project selection, a bounded recent-project list, and stable `cwd`-scoped thread listing, creation, and metadata reading without starting model turns or resuming thread history.

**Architecture:** A fixed Rust/Tauri command opens the official directory dialog, validates and registers the selected directory, and returns an opaque project ID plus display metadata. React receives no file contents and cannot submit an arbitrary cwd as authorization. Rust calls only stable App Server list/start/read APIs through a snapshot of the active connection. Seven small implementation PRs separate storage, selection, protocol, commands, frontend state, and the two UI surfaces.

**Tech Stack:** Rust 2024, Tauri 2, serde/serde_json, dunce, tauri-plugin-dialog, React 19, TypeScript 5.9, Vitest and Testing Library.

**Status:** Completed; the design and ADR were accepted in PR #22, and stages A2.1–A2.7 were implemented and merged through PR #35 (`f85e075b8a0ac38694d458ba3015e6d1bbaf0c9a`).

---

## Guardrails

- Work only under `apps/desktop` and `rivloom-docs`; do not modify `codex-rs`.
- Do not enable `capabilities.experimentalApi`, send `projectId`, or call `project/*`.
- Do not call `turn/start`; selecting/opening a project must consume no model quota.
- Do not call `thread/resume`; bounded history loading and subscriptions belong to A3.
- Rivloom project code must not read project file contents. App Server may still load project-local
  configuration or instructions when the user explicitly creates a thread.
- Do not log complete App Server payloads, thread history, account data, or directory contents.
- Keep recent projects at 20, thread pages at 50 or less, accumulated summaries at 500 or less, and
  opaque cursors at 4 KiB or less.
- Run `just fmt` in `apps/desktop` after code changes, then tests; do not accept unrelated CRLF churn.

## Stage A2.1 — Recent-project contracts and storage

### Task 1: Add project contracts and versioned recent-project storage

**Files:**

- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Create: `apps/desktop/src-tauri/src/project/mod.rs`
- Create: `apps/desktop/src-tauri/src/project/types.rs`
- Create: `apps/desktop/src-tauri/src/project/types_tests.rs`
- Create: `apps/desktop/src-tauri/src/project/storage.rs`
- Create: `apps/desktop/src-tauri/src/project/storage_tests.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Step 1: Add test-only filesystem support**

Add dev dependency `tempfile = "3"`. Do not add path-normalization dependencies, a database, or a
general settings framework in this PR. Regenerate the desktop-local Cargo lock through Cargo.

**Step 2: Write failing contract tests**

Use `pretty_assertions::assert_eq` on complete serialized values. Define this frontend contract:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalProject {
    pub id: String,
    pub path: String,
    pub name: String,
    pub last_opened_at: i64,
    pub availability: ProjectAvailability,
}
```

Cover `available`, `missing`, and `unreadable` as exhaustive camelCase enum values. Do not expose
storage version, temporary file names, or OS errors.

**Step 3: Run the new tests and verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::types::tests`

Expected: FAIL because the project module and contracts do not exist.

**Step 4: Implement the minimal contracts and storage file**

Store `{ "version": 1, "projects": [...] }` at
`app_local_data_dir/settings/recent-projects-v1.json`. Load missing files as empty. Reject unknown
future versions without overwriting them. Quarantine invalid JSON without logging its contents. Save
through a uniquely named same-directory temporary file, flush and sync before platform-specific
atomic replacement. On Windows use replace-existing semantics without deleting the destination
first; on Unix rename then best-effort sync the parent directory. Replacement failure must preserve
the old file and clean up the temporary file. Sort by `last_opened_at` descending, de-duplicate by
the persisted identity key, and truncate to 20. Require either the application's single-instance
guarantee or a cross-process storage lock before enabling writes.

**Step 5: Add storage behavior tests**

Test complete loaded lists for missing file, valid file, duplicate identities, more than 20 entries,
invalid JSON quarantine, unknown version, two consecutive saves, replacement failure preserving the
old file, temporary-file cleanup, and a parent path that makes saving fail. Assert that invalid data
is never echoed in returned errors.

**Step 6: Run the focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::`

Expected: PASS.

**Step 7: Commit**

```text
feat(desktop): add recent project storage
```

## Stage A2.2 — Backend directory selection and project registry

### Task 2: Select, validate, and manage recent projects

**Files:**

- Create: `apps/desktop/src-tauri/src/project/service.rs`
- Create: `apps/desktop/src-tauri/src/project/service_tests.rs`
- Create: `apps/desktop/src-tauri/src/project/state.rs`
- Modify: `apps/desktop/src-tauri/src/project/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`

**Step 1: Write failing service tests**

Use real temporary directories and whole-object assertions. Cover backend dialog cancellation,
absolute/canonical output, file-not-directory rejection, symlink/junction normalization where
supported, Windows case-insensitive and Unix exact identity, non-UTF-8 rejection without lossy
conversion, reopening updates recency without duplication, remove is idempotent, unreadable paths
produce zero App Server requests, and saved missing directories remain visible as unavailable.

**Step 2: Verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::service::tests`

Expected: FAIL because `ProjectService` is missing.

**Step 3: Implement minimal path validation**

Add direct `dunce = "1"`. A fixed Rust/Tauri command opens the official folder dialog and sends its
result directly to `ProjectService`; no command accepts an arbitrary path from React as proof of
selection. Use `dunce::canonicalize`, `metadata().is_dir()`, and an injected non-recursive
readability check. Reject paths that cannot be represented losslessly as App Server UTF-8 cwd.
Derive the display name from `file_name`, falling back to the normalized path for roots. Derive an
opaque project ID from the platform identity key and keep all OS detail in backend diagnostics.

**Step 4: Implement `ProjectState`**

Own a `Mutex<ProjectService>` created with the settings file path. Expose named methods
`list_recent`, `select_project`, `lookup_project`, and `remove_recent`; avoid ambiguous boolean parameters. A save
failure returns the opened project plus a nonfatal persistence warning rather than pretending the
directory itself failed.

**Step 5: Run focused tests and commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::`

Expected: PASS.

Commit: `feat(desktop): validate local project selection`

## Stage A2.3 — Stable thread core

### Task 3: Expose a nonblocking active App Server connection snapshot

**Files:**

- Modify: `apps/desktop/src-tauri/src/app_server/process.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/process_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/state.rs`

**Step 1: Write failing lifecycle tests**

Assert as complete state transitions that no connection is available before start, a clone is
available after successful initialization, the clone fails after shutdown/disconnect, and retry
returns a different connection identity.

**Step 2: Verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml app_server::process::tests`

Expected: FAIL because the production accessor does not exist.

**Step 3: Implement the smallest accessor**

Rename the current test-only `connection()` helper to a production-internal
`active_connection()` that clones `AppServerConnection`. Add
`AppServerState::active_connection() -> Option<Arc<dyn ConnectionControl>>`; acquire the supervisor
mutex only long enough to clone, then release it before any request.

Do not add another connection observer, notification observer, global event bus, or reset method.

**Step 4: Run process and connection tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml app_server::`

Expected: PASS.

**Step 5: Commit**

Commit: `feat(desktop): expose active app server connection`

### Task 4: Add stable cwd thread protocol adaptation

**Files:**

- Create: `apps/desktop/src-tauri/src/project/protocol.rs`
- Create: `apps/desktop/src-tauri/src/project/protocol_tests.rs`
- Create: `apps/desktop/src-tauri/src/project/thread_service.rs`
- Create: `apps/desktop/src-tauri/src/project/thread_service_tests.rs`
- Modify: `apps/desktop/src-tauri/src/project/mod.rs`
- Modify: `apps/desktop/src-tauri/src/project/types.rs`

**Step 1: Write failing protocol tests**

For fake connection requests, deep-compare exact method and params:

```json
{"method":"thread/list","params":{"cwd":"<normalized>","limit":50,"sortKey":"recency_at","sortDirection":"desc"}}
{"method":"thread/start","params":{"cwd":"<normalized>"}}
{"method":"thread/read","params":{"threadId":"thr-1","includeTurns":false}}
```

Cover pagination cursor pass-through, field whitelist parsing, missing required fields, malformed
timestamps, sanitized remote errors, disconnect, response `cwd` mismatch, a response over 50 items,
a cursor over 4 KiB, UTF-8-safe bounding of long preview/name fields, and a 500-item
accumulated-summary boundary.
Assert recorded method names contain no `thread/resume`, `turn/start`, or `project/` prefix.

**Step 2: Verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::`

Expected: FAIL because adapters are missing.

**Step 3: Implement normalized thread types**

Return only `id`, `name`, `preview`, timestamps, status, and normalized `cwd`. Parse response data
from `serde_json::Value` in the project protocol module and discard turns, path, unknown fields, and
experimental project metadata.

**Step 4: Implement thread service methods**

Use named methods `list_threads`, `start_thread`, and `read_thread`. Every method receives a project
resolved from the backend registry and a connection snapshot. Verify returned cwd equality before
returning normalized data. Limit requests and responses to 50, reject oversized cursors, truncate
display-only preview/name fields on UTF-8 boundaries, and stop pagination after 500 accumulated
summaries.

**Step 5: Run project and App Server tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::`

Run: `cargo test --manifest-path src-tauri/Cargo.toml app_server::`

Expected: PASS.

**Step 6: Commit**

Commit: `feat(desktop): add bounded cwd scoped thread service`

Tasks 3 and 4 form one reviewable PR because the connection accessor has no standalone consumer.

## Stage A2.4 — Fixed Tauri project commands

### Task 5: Register backend dialog integration and project commands

**Files:**

- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/src/project/commands.rs`
- Create: `apps/desktop/src-tauri/src/project/commands_tests.rs`

**Step 1: Add official dialog dependencies**

Add a matching Tauri 2 version of the Rust `tauri-plugin-dialog` crate and register it. The fixed
backend selection command owns the dialog interaction; do not add the JavaScript dialog package,
frontend `dialog:allow-open`, or filesystem plugin permissions.

**Step 2: Write failing command tests**

Cover `list_recent_projects`, `select_project`, `remove_recent_project`, `list_project_threads`,
`start_project_thread`, and `read_project_thread`. Disconnected thread
commands must return one safe error contract; local recent-project commands must still work.
Assert commands accept only opaque project IDs after selection and that an injected arbitrary path
cannot be used to create an authorization record.

**Step 3: Implement commands with blocking work off the UI thread**

Resolve `ProjectState` and `AppServerState` from `AppHandle` inside `spawn_blocking`. Look up the
opaque project ID and validate its registered path again on every App Server operation. Never accept
a caller-provided path or cwd after the backend dialog selection.

**Step 4: Register state and handlers**

Create `settings` beside `codex-home`, manage `ProjectState`, and register all six commands in
`generate_handler!`. Preserve current account initialization and shutdown order.

**Step 5: Run Rust tests and checks**

Run: `just test-rust`

Run: `just check-rust`

Expected: all tests and checks PASS.

**Step 6: Commit**

Commit: `feat(desktop): expose local project commands`

## Stage A2.5 — Typed React boundary

### Task 6: Add typed frontend bridge and race-safe hooks

**Files:**

- Create: `apps/desktop/src/types/project.ts`
- Create: `apps/desktop/src/lib/projectBridge.ts`
- Create: `apps/desktop/src/lib/projectBridge.test.ts`
- Create: `apps/desktop/src/hooks/useRecentProjects.ts`
- Create: `apps/desktop/src/hooks/useRecentProjects.test.tsx`
- Create: `apps/desktop/src/hooks/useProjectThreads.ts`
- Create: `apps/desktop/src/hooks/useProjectThreads.test.tsx`

**Step 1: Write failing bridge tests**

Mock only fixed Tauri invokes. Assert `select_project` cancellation leaves state unchanged and never
returns an unregistered path; every other command sends a project ID and fixed camelCase params.
Use complete object comparisons for returned project/thread pages.

**Step 2: Verify failure**

Run: `corepack pnpm test src/lib/projectBridge.test.ts`

Expected: FAIL because the bridge is missing.

**Step 3: Implement bridge and discriminated contracts**

Export no raw `invoke`, generic method caller, direct dialog API, path-accepting open method, or cwd
parameter. Model loading, ready, empty, and error states as discriminated unions instead of boolean
combinations.

**Step 4: Write and implement hook race tests**

Cover runtime disconnect/reconnect, project A response arriving after switching to project B,
duplicate actions, pagination append, refresh replacement, and unmount. Use lifecycle revisions like
`useAccountStatus`; stale results must not overwrite current state.

**Step 5: Run focused frontend tests**

Run: `corepack pnpm test src/lib/projectBridge.test.ts src/hooks/useRecentProjects.test.tsx src/hooks/useProjectThreads.test.tsx`

Expected: PASS.

**Step 6: Commit**

Commit: `feat(desktop): add local project frontend bridge`

## Stage A2.6 — Recent-project home

### Task 7: Build the recent-project home

**Files:**

- Create: `apps/desktop/src/components/ProjectAccessCard/ProjectAccessCard.tsx`
- Create: `apps/desktop/src/components/ProjectAccessCard/ProjectAccessCard.module.css`
- Create: `apps/desktop/src/components/ProjectAccessCard/ProjectAccessCard.test.tsx`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/app/App.module.css`
- Modify: `apps/desktop/src/content/zh-CN.ts`

**Step 1: Write failing user-flow tests and snapshots**

Cover signed-out gating, open button, dialog cancel, empty recent list, available project open,
missing/unreadable disabled rows, remove, storage warning, keyboard labels, and long path rendering.
Add intentional snapshots for empty, populated, and unavailable states.

**Step 2: Verify failure**

Run: `corepack pnpm test src/components/ProjectAccessCard/ProjectAccessCard.test.tsx src/app/App.test.tsx`

Expected: FAIL because project UI is missing.

**Step 3: Implement the minimal home UI**

Preserve account and service cards. Change stage copy to local projects only when connected and
signed in. Keep one primary folder action, a semantic recent list, explicit unavailable labels, and
an accessible remove button that does not also open the project.

**Step 4: Run focused tests and commit**

Run the command from Step 2.

Expected: PASS with reviewed snapshots.

Commit: `feat(desktop): add recent project home`

## Stage A2.7 — Bounded thread workspace

### Task 8: Build the project workspace and thread list

**Files:**

- Create: `apps/desktop/src/components/ProjectWorkspace/ProjectWorkspace.tsx`
- Create: `apps/desktop/src/components/ProjectWorkspace/ProjectWorkspace.module.css`
- Create: `apps/desktop/src/components/ProjectWorkspace/ProjectWorkspace.test.tsx`
- Create: `apps/desktop/src/components/ThreadList/ThreadList.tsx`
- Create: `apps/desktop/src/components/ThreadList/ThreadList.module.css`
- Create: `apps/desktop/src/components/ThreadList/ThreadList.test.tsx`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/content/zh-CN.ts`

**Step 1: Write failing workspace tests and snapshots**

Cover load, empty, failure, retry, bounded pagination, new thread, metadata read, cwd mismatch error,
runtime disconnect, return home, timestamp fallback, active status, keyboard selection, and long
preview/path wrapping. Assert no test bridge call records `thread/resume`, `turn/start`, `project/*`,
an arbitrary path, or cwd.

**Step 2: Verify failure**

Run: `corepack pnpm test src/components/ProjectWorkspace/ProjectWorkspace.test.tsx src/components/ThreadList/ThreadList.test.tsx src/app/App.test.tsx`

Expected: FAIL because workspace components are missing.

**Step 3: Implement project workspace**

Show a bounded thread list and “load more” only when `nextCursor` exists and fewer than 500 summaries
are loaded. New thread requires a direct user click. Existing thread selection calls read only and
displays its normalized summary plus an explicit A3 chat/resume placeholder; do not resume the
thread or render/store response turns.

**Step 4: Run focused tests and commit**

Run the command from Step 2.

Expected: PASS with reviewed snapshots.

Commit: `feat(desktop): add cwd scoped thread workspace`

### Task 9: Final verification and documentation

**Files:**

- Create: `rivloom-docs/plans/2026-08-27-desktop-local-projects-and-threads-verification.md`
- Update local excluded file: `rivloom-docs/CURRENT_STATUS.md`

**Step 1: Format once after code changes**

Run: `just fmt`

Review `git diff --stat` and `git diff --check`. Reject unrelated line-ending changes.

**Step 2: Run the desktop regression suite**

Run: `just check`

Run: `just test-rust`

Run: `just check-rust`

Expected baseline before A2: 8 frontend files/40 tests, 111 Rust tests, TypeScript and Vite build
PASS. Expected A2 counts must be recorded from the actual run, not predicted.

**Step 3: Run deterministic protocol verification**

Use the fake connection/harness to select, list, start, and read a project thread. Record only
method names and sanitized outcomes. Assert zero `turn/start`, `project/*`, account methods, OAuth
values, Rivloom project-service file reads, and model calls. Also assert zero `thread/resume`, and
record that App Server may independently load project-level configuration on `thread/start`.

**Step 4: Perform deterministic visual checks**

Check `1180×760` and `960×640`: empty recent projects, populated list with long Windows paths,
unavailable project, empty thread list, populated thread list, disconnected and error states. Do not
include real usernames or private paths in committed screenshots.

**Step 5: Review change size and boundaries**

Keep each PR below 800 changed lines and complex logic below 500. Confirm no `codex-rs`, App Server
schema, experimental initialization, credential, log, binary, `target`, or `dist` changes.

**Step 6: Update verification and status documents**

Record actual commits, tests, visual states, known gaps, and the next unique priority. Keep
`CURRENT_STATUS.md` local/excluded.

**Step 7: Final commits and handoff**

Commit verification separately as `docs: record A2 local project verification`. Push and create a
PR only after the user approves the implementation result; do not merge without fresh confirmation.

## Execution handoff after review

Once the design and ADR are accepted, execute this plan in fresh stage worktrees using the
`executing-plans` skill. Complete A2.1 first and review its actual diff before creating A2.2. Each
stage is a separate reviewable PR except Tasks 3 and 4, which land together as A2.3. Do not start
dependent stages in parallel because they share `lib.rs`, package locks, `App.tsx`, and the project
contracts. Keep each PR below 800 changed lines and complex logic below 500; split a stage again if
its actual diff crosses either boundary.
