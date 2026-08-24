# Rivloom Desktop Shell Implementation Plan

Plan status: validated and ready for task-by-task execution.

> **For Codex:** Use `$executing-plans` to execute this plan task-by-task. After each task, run its verification, report the result, and wait for user approval before starting the next task.

**Goal:** Build the first Windows Rivloom Desktop shell with the validated visual system and a supervised stdio handshake to the bundled Codex App Server.

**Architecture:** React and TypeScript render a Vite SPA inside Tauri. A narrow Rust backend owns the App Server sidecar, isolated `CODEX_HOME`, JSONL handshake, lifecycle, and sanitized runtime status. The webview receives only typed status DTOs through Tauri IPC and events.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vite, CSS Modules, CSS custom properties, Vitest, Testing Library, pnpm.

---

## Preconditions

- Work only in `C:\project\opencohive\.worktrees\desktop-shell`.
- Branch must be `feat/desktop-shell` at or after `5dd1d9249a`.
- Do not modify `codex-core` or App Server protocol in this milestone.
- Do not call a model or consume account quota.
- Use the existing App Server build for the first manual handshake when available.
- Keep generated binaries, `node_modules`, `dist`, Cargo `target`, screenshots, and logs out of Git.
- Use the visual contract in `rivloom-docs/plans/2026-08-24-desktop-shell-design.md`.

## Task 1: Register the desktop workspace and scaffold the frontend

**Files:**

- Modify: `pnpm-workspace.yaml`
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/tsconfig.app.json`
- Create: `apps/desktop/tsconfig.node.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/vite-env.d.ts`
- Create: `apps/desktop/src/test/setup.ts`
- Create: `apps/desktop/justfile`
- Modify: `pnpm-lock.yaml` through pnpm only

### Step 1.1: Add the workspace path

Add `apps/*` to `pnpm-workspace.yaml`. Keep all existing supply-chain policy fields unchanged.

### Step 1.2: Create the package manifest

Use package name `@rivloom/desktop`, version `0.1.0-alpha.0`, and `private: true`.

Required scripts:

```json
{
  "dev": "vite",
  "build": "tsc -b && vite build",
  "test": "vitest run",
  "test:watch": "vitest",
  "format": "prettier --check .",
  "format:fix": "prettier --write .",
  "tauri": "tauri"
}
```

Runtime dependencies:

```text
@tauri-apps/api
react
react-dom
```

Development dependencies:

```text
@tauri-apps/cli
@testing-library/jest-dom
@testing-library/react
@testing-library/user-event
@types/react
@types/react-dom
@vitejs/plugin-react
jsdom
typescript
vite
vitest
```

Install through pnpm so the root lockfile and minimum-release-age policy select and pin the actual
versions. Do not use npm, yarn, Bun, or a second lockfile.

### Step 1.3: Add the failing frontend smoke test

Create `apps/desktop/src/app/App.test.tsx` with an initial test that imports `App` and expects the
Rivloom product heading to be present. Run it before creating `App.tsx`.

Run:

```text
pnpm --filter @rivloom/desktop test
```

Expected: FAIL because `src/app/App.tsx` does not exist.

### Step 1.4: Add the minimum React entry point

Create only enough `App.tsx` to render a semantic `main` and `h1` with `Rivloom`; do not add final
styling yet.

### Step 1.5: Verify the scaffold

Run:

```text
pnpm --filter @rivloom/desktop test
pnpm --filter @rivloom/desktop build
```

Expected: both commands pass and `apps/desktop/dist` is generated but ignored.

### Step 1.6: Commit

```text
git add pnpm-workspace.yaml pnpm-lock.yaml apps/desktop
git commit -m "build(desktop): scaffold React workspace"
```

Do not push yet.

## Task 2: Implement the validated design system and shell layout

**Files:**

- Create: `apps/desktop/src/styles/tokens.css`
- Create: `apps/desktop/src/styles/global.css`
- Create: `apps/desktop/src/content/zh-CN.ts`
- Create: `apps/desktop/src/components/AppShell/AppShell.tsx`
- Create: `apps/desktop/src/components/AppShell/AppShell.module.css`
- Create: `apps/desktop/src/components/StatusBadge/StatusBadge.tsx`
- Create: `apps/desktop/src/components/StatusBadge/StatusBadge.module.css`
- Create: `apps/desktop/src/components/ServiceStatusCard/ServiceStatusCard.tsx`
- Create: `apps/desktop/src/components/ServiceStatusCard/ServiceStatusCard.module.css`
- Create: `apps/desktop/src/components/ServiceStatusCard/ServiceStatusCard.test.tsx`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/main.tsx`

### Step 2.1: Write behavior tests first

Test the UI contract rather than CSS internals:

- The app exposes navigation and main-content landmarks.
- Starting state says the core service is starting.
- Connected state exposes version and platform text.
- Error state has `role="alert"` and an enabled retry button.
- Status text is present independently of its colored indicator.

Run the focused tests and confirm they fail because the components are missing.

### Step 2.2: Add semantic runtime status types

The component input should be a discriminated union rather than booleans:

```ts
export type RuntimeStatus =
  | { state: "starting" }
  | {
      state: "connected";
      appVersion: string;
      appServerUserAgent: string;
      platform: string;
      codexHome: string;
    }
  | { state: "error"; message: string; retryable: boolean }
  | { state: "stopped" };
```

Create this in `apps/desktop/src/types/runtime.ts` and import it from the components.

### Step 2.3: Implement tokens and layout

Copy the approved semantic colors, spacing, radii, fonts and motion values from the design document.
Use native semantic elements and CSS Grid. Do not add Tailwind, a component library, remote fonts,
custom title-bar controls, decorative animation, or a dark-mode toggle.

### Step 2.4: Verify accessibility behavior

Use Testing Library role and label queries. Confirm the error message is announced, retry is keyboard
reachable, and no test relies on a CSS class or test ID when a semantic query exists.

### Step 2.5: Verify and commit

Run:

```text
pnpm --filter @rivloom/desktop test
pnpm --filter @rivloom/desktop build
pnpm --filter @rivloom/desktop format
```

Commit:

```text
git add apps/desktop/src
git commit -m "feat(desktop): add Rivloom shell design system"
```

## Task 3: Scaffold the Tauri backend with minimum permissions

**Files:**

- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: `apps/desktop/src-tauri/build.rs`
- Create: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/capabilities/default.json`
- Create: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/src/runtime_status.rs`
- Modify: `apps/desktop/justfile`

### Step 3.1: Create Rust status tests first

Define tests in `apps/desktop/src-tauri/src/runtime_status_tests.rs` and attach them with an explicit
`#[path = "runtime_status_tests.rs"]` test module. Verify serialized JSON uses camelCase and contains
only the fields required by the frontend `RuntimeStatus` union.

Run `just test-rust` from `apps/desktop` and confirm it fails before the Rust types exist.

### Step 3.2: Add the independent Tauri crate

Use Tauri 2 and keep the crate outside the upstream `codex-rs` Cargo workspace. Add only:

```text
tauri
serde with derive
serde_json
thiserror
```

Add `tauri-build` as a build dependency. Do not add a broad shell permission or network listener.

### Step 3.3: Add the minimum application configuration

Use:

```text
productName: Rivloom
identifier: com.rivloom.desktop
version: 0.1.0-alpha.0
default size: 1180 x 760
minimum size: 960 x 640
decorations: true
frontendDist: ../dist
devUrl: http://localhost:5173
```

The default capability may expose only core window/event functionality and Rivloom's explicit Tauri
commands. Do not expose `shell:allow-spawn` to the webview.

### Step 3.4: Keep entry points thin

`main.rs` only calls `rivloom_desktop_lib::run()`. `lib.rs` owns builder setup. Place serializable
status types in `runtime_status.rs`; do not mix process handling into `lib.rs`.

### Step 3.5: Verify and commit

Run from `apps/desktop`:

```text
just fmt
just test-rust
just check-rust
```

Also run `just fmt` from `codex-rs` as required by the repository after Rust changes.

Commit:

```text
git add apps/desktop/src-tauri apps/desktop/justfile
git commit -m "build(desktop): add minimal Tauri backend"
```

## Task 4: Implement tested App Server JSONL initialization

**Files:**

- Create: `apps/desktop/src-tauri/src/app_server/mod.rs`
- Create: `apps/desktop/src-tauri/src/app_server/protocol.rs`
- Create: `apps/desktop/src-tauri/src/app_server/protocol_tests.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

### Step 4.1: Write protocol tests first

Cover these exact behaviors:

1. Initialization request uses ID `0` and client info:

```json
{
  "method": "initialize",
  "id": 0,
  "params": {
    "clientInfo": {
      "name": "rivloom_desktop",
      "title": "Rivloom Desktop",
      "version": "0.1.0-alpha.0"
    }
  }
}
```

2. The initialized notification is exactly one JSONL message:

```json
{"method":"initialized","params":{}}
```

3. A successful response extracts `userAgent`, `codexHome`, `platformFamily`, and `platformOs`.
4. Error responses become a typed protocol error.
5. Invalid JSON becomes a parse error without panic.
6. A response with the wrong ID is rejected.

Run the focused Rust test and confirm it fails before implementation.

### Step 4.2: Implement the minimum protocol module

Use serde structs and enums; do not build JSON with interpolated strings. Keep raw protocol types
private and convert successful initialization into `RuntimeStatus::Connected`.

Do not add thread, turn, account, approval, proxy, or experimental protocol types in this milestone.

### Step 4.3: Verify and commit

Run:

```text
just fmt
just test-rust
just check-rust
```

Commit:

```text
git add apps/desktop/src-tauri/src
git commit -m "feat(desktop): add App Server initialization protocol"
```

## Task 5: Prepare and supervise the App Server sidecar

**Files:**

- Create: `apps/desktop/scripts/prepare-app-server.mjs`
- Create: `apps/desktop/src-tauri/binaries/.gitignore`
- Create: `apps/desktop/src-tauri/src/app_server/process.rs`
- Create: `apps/desktop/src-tauri/src/app_server/process_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/package.json`

### Step 5.1: Test the preparation script's pure path logic

Factor target-triple naming and source selection into exported pure functions. Test:

- Windows output ends in `codex-app-server-<target-triple>.exe`.
- `RIVLOOM_APP_SERVER_PATH` wins when present.
- Missing source reports an actionable error.
- The generated binary directory ignores binaries but retains its `.gitignore`.

Do not copy a binary until these tests pass.

### Step 5.2: Implement sidecar preparation

The script should:

1. Read optional `RIVLOOM_APP_SERVER_PATH`.
2. Otherwise locate `codex-rs/target/{debug|release}/codex-app-server.exe` inside the current worktree.
3. Ask `rustc --print host-tuple` for the target triple.
4. Copy, never move, the source executable into `src-tauri/binaries` with Tauri's required suffix.
5. Never commit the copied executable.

For the first local run, use the already verified binary at
`C:\project\opencohive\codex-rs\target\debug\codex-app-server.exe` through the environment override.
Do not hardcode that developer-specific path in tracked files.

### Step 5.3: Add the process supervisor tests

Abstract process launch and line transport behind private traits so unit tests can use a fake child.
Test state transitions:

```text
stopped -> starting -> connected
stopped -> starting -> error
connected -> stopped on shutdown
error -> starting on manual retry
```

Also test that the child receives the initialization request followed by the initialized notification,
and that an initialization timeout produces an error status.

### Step 5.4: Implement the supervisor

Use `tauri-plugin-shell` from Rust to launch only the configured sidecar. Set `CODEX_HOME` to the
Rivloom local application data directory's `codex-home` child. Parse stdout as buffered JSONL and
write sanitized stderr diagnostics.

The React capability file must not receive shell spawn or execute permissions. Process handles stay
inside Rust state. Closing the application must close input and terminate a still-running sidecar.

### Step 5.5: Verify the real no-quota handshake

Prepare the known App Server binary, run Tauri, and verify the UI reaches `connected` without making
an account or model request. Confirm the displayed `codexHome` is under Rivloom's application data
directory.

Close Rivloom and verify no child `codex-app-server.exe` remains from that run.

### Step 5.6: Verify and commit

Run the frontend script tests, Rust tests, checks, and formatting. Then commit:

```text
git add apps/desktop
git commit -m "feat(desktop): supervise bundled App Server"
```

## Task 6: Connect the React shell to typed Tauri status

**Files:**

- Create: `apps/desktop/src/lib/runtimeBridge.ts`
- Create: `apps/desktop/src/lib/runtimeBridge.test.ts`
- Create: `apps/desktop/src/hooks/useRuntimeStatus.ts`
- Create: `apps/desktop/src/hooks/useRuntimeStatus.test.tsx`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/components/ServiceStatusCard/ServiceStatusCard.tsx`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

### Step 6.1: Write bridge and hook tests first

Mock Tauri `invoke` and `listen`. Verify:

- Initial status is read once.
- Later status events update the component.
- Listener cleanup runs on unmount.
- Retry invokes only the named Rivloom retry command.
- Repeated retry clicks are disabled while starting.

### Step 6.2: Implement the narrow bridge

Expose only:

```ts
getRuntimeStatus(): Promise<RuntimeStatus>
retryAppServer(): Promise<RuntimeStatus>
onRuntimeStatusChanged(listener): Promise<UnlistenFn>
```

Do not expose generic command names, shell arguments, executable paths, environment variables, or raw
App Server messages to React.

### Step 6.3: Add the matching Tauri commands

The Rust commands return `RuntimeStatus` and trigger the private supervisor. Validate command state
server-side; do not trust frontend status.

### Step 6.4: Verify and commit

Run frontend and Rust tests, type checking, formatting, and production builds. Commit:

```text
git add apps/desktop
git commit -m "feat(desktop): display live App Server status"
```

## Task 7: Visual, lifecycle, and repository verification

**Files:**

- Modify only files required to fix verified defects.
- Add: `rivloom-docs/plans/2026-08-24-desktop-shell-verification.md`

### Step 7.1: Run automated checks

From `apps/desktop`:

```text
just fmt
just test
just check
```

From `codex-rs`:

```text
just fmt
```

Expected: all scoped checks pass. Do not run the full upstream Codex test suite without asking the
user first.

### Step 7.2: Perform visual QA

Run the Vite frontend with deterministic mocked starting, connected, and error states. Capture and
inspect each state at:

```text
1180 x 760
960 x 640
```

Check:

- No clipping or unexpected scrollbars.
- Typography renders Chinese and code samples correctly.
- Focus ring is visible.
- Status is understandable without relying on color.
- Error state includes a recovery action.
- The interface matches the approved fresh, quiet visual direction.

Fix defects before presenting screenshots.

### Step 7.3: Perform native lifecycle QA

Start the Tauri app with the real sidecar and verify:

1. Window opens and reports `starting`.
2. Handshake reaches `connected`.
3. Version and platform details match the response.
4. `CODEX_HOME` is Rivloom-owned.
5. Closing the window removes the child process.
6. Missing sidecar produces the approved error state.
7. Retry recovers after the sidecar is restored.

### Step 7.4: Record evidence and commit

Write the exact commands, results, known limitations, and screenshot paths to the verification file.
Do not commit temporary screenshots unless they are intentionally added as documentation assets.

Commit:

```text
git add apps/desktop rivloom-docs/plans/2026-08-24-desktop-shell-verification.md
git commit -m "test(desktop): verify shell lifecycle and UI"
```

## Task 8: Review and delivery checkpoint

### Step 8.1: Review the complete diff

Confirm:

- No unplanned edits under `codex-rs`.
- No generated binaries, logs, secrets, tokens, or user-specific paths are tracked.
- `pnpm-workspace.yaml` retains upstream security policy.
- All dependencies are necessary for this milestone.
- No generic shell capability is exposed to React.
- Commit history is split by coherent, testable steps.

### Step 8.2: Report to the user

Provide:

- Completed capabilities.
- Test and visual verification evidence.
- Known limitations.
- Exact commits on `feat/desktop-shell`.
- Confirmation that no model call or quota was used.

Wait for explicit approval before pushing the branch or opening a GitHub pull request.
