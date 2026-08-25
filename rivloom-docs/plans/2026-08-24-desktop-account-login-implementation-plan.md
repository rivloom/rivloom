# Rivloom Desktop Account Login Implementation Plan

Plan status: A1.1, A1.2a-1 and A1.2a-2a1 merged and verified; A1.2a-2a2 was split after local
review, and A1.2a-2a2a is in progress.

> **For Codex:** Use `$executing-plans` task-by-task. After every task, run its verification, report
> the result, and wait for user approval before continuing.

**Goal:** Build a bounded long-lived App Server connection and complete ChatGPT browser/device-code
login lifecycle without exposing credentials or calling a model.

**Architecture:** Tauri owns the sidecar, protocol, login attempt and browser URLs. A generic
`AppServerConnection` correlates requests and routes notifications to `AccountService`; React receives
only typed state through fixed commands and one event.

**Tech Stack:** Tauri 2, Rust 2024, serde/serde_json, tauri-plugin-shell, React 19, TypeScript,
Vitest, Testing Library, CSS Modules, pnpm.

---

## Preconditions

- A1.1 (Tasks 1–3) is already merged into `main` through `812c27ffa9`; treat it as the verified
  baseline and do not reimplement it.
- A1.2a-1 was merged through PR #9; use merge commit `68b8bdf92f` as the verified account-read
  baseline.
- A1.2a-2a1 was merged through PR #10; use merge commit `ec57d275c5` as the verified login-safety
  baseline.
- For A1.2a-2a2a, work only in
  `C:\project\opencohive\.worktrees\desktop-account-browser-start-cleanup-a12a2a` on
  `codex/desktop-account-browser-start-cleanup-a12a2a`, created from `ec57d275c5`.
- Create fresh branches and worktrees for A1.2a-2a2b, A1.2a-2a3, A1.2a-2b, A1.2b and A1.2c only
  after the preceding PR is merged and the user explicitly approves the next setup step.
- Follow `2026-08-24-desktop-account-login-design.md` and the repository `AGENTS.md`.
- Do not modify `codex-rs`, App Server protocol, `CODEX_SANDBOX_*`, or upstream docs.
- Do not create a thread, send a turn, call a model, inspect credentials, or log sensitive values.
- Keep React without generic App Server, URL, shell, executable, path or environment access.
- Do not push, create a PR, merge, or run the complete upstream test suite without approval.

## Delivery checkpoints

- **A1.1 — complete:** Tasks 1–3, protocol foundation, merged through PRs #4–#8 and verified on
  `main`.
- **A1.2a-1 — Account Read Core:** Task 4A only. Deliver account types, crate-internal request access,
  connection state and `account/read` mapping. Do not add login attempts, URLs, notifications,
  Tauri commands, React code or UI.
- **A1.2a-2a1 — Login Safety Primitives:** Task 4B1 only. Deliver typed login/cancel parsing, the
  narrow URL opener contract and official URL validation. Do not start login or open a browser.
- **A1.2a-2a2a — Browser Start & Cleanup:** Task 4B2a only. Deliver fixed browser starts,
  approved URL opening and recoverable sequential-attempt cleanup.
- **A1.2a-2a2b — Lifecycle Concurrency:** Task 4B2b only. Deliver serialized concurrent starts,
  stale-read and stale-connection protection, and defensive mismatched-response coverage.
- **A1.2a-2a3 — Device Code & Switching:** Task 4B3 only. Deliver device-code starts, controlled
  verification opening, retry recovery and browser/device switching.
- **A1.2a-2b — Completion & Account Actions:** Task 4B4 only. Deliver matching notifications,
  background refresh, explicit cancel and logout behavior.
- **A1.2b — Account Bridge:** Tasks 5–6 only. Deliver six fixed Tauri commands, one normalized event,
  the typed React bridge and race-safe Hook.
- **A1.2c — Account UI:** Tasks 7–8 only. Deliver all six UI states, device-code interactions,
  accessible logout confirmation and final verification evidence.
- Each A1.2 checkpoint is a separate reviewable PR. Stop for local verification and user approval
  before every commit, push, PR creation and merge.
- Keep complex logic changes below 500 changed lines and every non-mechanical PR below 800; stop and
  propose a smaller coherent split before crossing either review threshold.

## Completed A1.1 baseline — do not re-execute

## Task 1: Add bounded wire parsing — completed

**Files:**

- Create: `apps/desktop/src-tauri/src/app_server/wire.rs`
- Create: `apps/desktop/src-tauri/src/app_server/wire_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify through Cargo: `apps/desktop/src-tauri/Cargo.lock`

### Steps

1. Add `pretty_assertions` as a dev dependency because repository tests require it. Refresh the
   independent desktop lock through Cargo, then run `just bazel-lock-update` at repository root and
   verify it does not create unrelated lock drift.
2. Write failing sibling tests for result/error responses, notifications, server requests, invalid
   shape, non-integer client response ID, split chunks, multiple lines, CRLF, invalid UTF-8 and an
   injected over-limit buffer.
3. Run:
   `cargo test --manifest-path src-tauri/Cargo.toml app_server::wire::tests`
   and confirm failure because the module is absent.
4. Implement private `InboundMessage` classification with serde/`Value`, plus a byte-level
   `JsonLineDecoder`. Production maximum: 4 MiB. Errors must not display params/results.
5. Re-run the focused test and
   `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.
6. Review that the module is account-agnostic and below 500 lines. Wait for approval, then commit
   `feat(desktop): decode App Server messages`.

## Task 2: Route requests and notifications — completed

**Files:**

- Create: `apps/desktop/src-tauri/src/app_server/connection.rs`
- Create: `apps/desktop/src-tauri/src/app_server/connection_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`

### Steps

1. Define documented private `ConnectionControl` and `NotificationObserver` traits; avoid
   `async_trait` and use explicit `Send` futures only if async is necessary.
2. Write failing tests: IDs start at 1, reversed responses reach correct callers, notifications do
   not consume pending responses, remote error is sanitized, timeout/write failure removes pending,
   65th pending request is rejected, disconnect fails each waiter once, duplicate/unknown IDs do not
   panic, and server requests receive method-not-supported.
3. Run the focused connection test and confirm failure.
4. Implement `AppServerConnection` with `AtomicU64`, a mutex-protected pending map, per-request
   completion channel, 64-request limit and injected/default 10-second timeout. Register before write
   and remove on every terminal path.
5. Run focused tests, `just test-rust`, `just check-rust`, and `just fmt` from `apps/desktop`.
6. Wait for approval, then commit `feat(desktop): route App Server requests`.

## Task 3: Keep the sidecar connection alive — completed

**Files:**

- Create: `apps/desktop/src-tauri/src/app_server/transport.rs`
- Create: `apps/desktop/src-tauri/src/app_server/transport_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/process.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/process_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/protocol.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`

### Steps

1. Add failing lifecycle tests: initialization remains ID 0; post-init response/notification is
   consumed; termination fails pending and enters runtime error; retry uses a new connection;
   shutdown stops reader and terminates once.
2. Run `cargo test --manifest-path src-tauri/Cargo.toml app_server::process::tests` and confirm new
   post-init cases fail.
3. Mechanically extract launcher, control, command events, stderr and decoder from `process.rs`.
   Reader owns event receiving; shared control uses safe interior mutability.
4. After initialized, start one reader and store a cloneable connection handle. Never hold the
   supervisor lock while waiting for an RPC response.
5. Run `just test-rust`, `just check-rust`, and `just fmt`. Confirm `process.rs` shrinks and no
   `codex-rs` file changes.
6. At A1.1, run `git diff --check`, `git diff --stat 584ba0a7c4`, and review the complete diff.
   Stop for user decision about one PR versus an A1.1 split. After approval, commit
   `refactor(desktop): keep App Server connection alive`.

## A1.2a-1 — Account Read Core

## Task 4A: Implement account state and read service

**Files:**

- Create: `apps/desktop/src-tauri/src/account/mod.rs`
- Create: `apps/desktop/src-tauri/src/account/types.rs`
- Create: `apps/desktop/src-tauri/src/account/types_tests.rs`
- Create: `apps/desktop/src-tauri/src/account/service.rs`
- Create: `apps/desktop/src-tauri/src/account/service_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/connection.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

### Steps

1. Create the private `account` module and write failing sibling tests that deep-compare all six
   `AccountStatus` variants and their camelCase serialization. Assert the public state cannot contain
   tokens, login IDs, raw errors, environment values or filesystem paths.
2. Run the focused account type test and confirm it fails because the types are not implemented.
3. Implement only the approved `checking`, `signedOut`, `browserPending`, `devicePending`, `signedIn`
   and `error` variants. Keep email nullable; unsupported account/auth types become a safe,
   non-retryable error.
4. Re-run the focused type tests and confirm they pass.
5. With a fake `ConnectionControl`, write failing read-service tests for auth-required null,
   auth-not-required null, ChatGPT/unsupported accounts, nullable email, malformed responses,
   sanitized request failures, disconnect cleanup and reconnect.
6. Run the focused service tests and confirm the missing read service behavior fails.
7. Promote only `ConnectionControl` and `ConnectionError` to crate-internal visibility and re-export
   them privately from `app_server`; do not make the connection module or these types public.
8. Implement `AccountService` connection binding, status snapshots and `account/read` mapping. Never
   hold the service state lock while waiting for a request, and never infer a failed read as signed out.
9. Expose no login attempt, URL, notification observer, Tauri command, event or frontend API in
   A1.2a-1.
10. Run focused account tests, then `just test-rust`, `just check-rust`, and `just fmt` from
    `apps/desktop`. Run `git diff --check` and review the diff against `812c27ffa9`.
11. Stop for user review. After explicit approval, commit
    `feat(desktop): read ChatGPT account state`. Do not push or create a PR yet.

## A1.2a-2a1 — Login Safety Primitives

## Task 4B1: Add typed login and URL safety primitives

**Files:**

- Create: `apps/desktop/src-tauri/src/account/login.rs`
- Create: `apps/desktop/src-tauri/src/account/login_tests.rs`
- Modify: `apps/desktop/src-tauri/src/account/mod.rs`

### Steps

1. Write failing sibling tests for browser/device response parsing, unknown/malformed payloads,
   cancel confirmations and official URL validation.
2. Define a documented `UrlOpener` trait with one object-safe method. Implement no opener and grant no
   Tauri capability in this checkpoint.
3. Parse only the expected camelCase response fields. Unknown variants must discard their payload;
   malformed values must become `None` without logging or returning raw data.
4. Allow HTTPS on `chatgpt.com`, `openai.com` and real subdomains only; reject credentials,
   non-default ports and lookalike suffixes.
5. Run focused and full desktop Rust checks, review size, and stop for user approval before commit,
   push or PR creation. Suggested commit: `feat(desktop): validate ChatGPT login URLs`.

## A1.2a-2a2a — Browser Start & Cleanup

## Task 4B2a: Start and clean up browser login safely

**Files:**

- Modify: `apps/desktop/src-tauri/src/account/service.rs`
- Modify: `apps/desktop/src-tauri/src/account/service_tests.rs`

### Steps

1. Write failing tests for fixed browser params, invalid URLs, opener failures, failed cleanup
   retention and unavailable or malformed start responses.
2. Implement one active browser attempt and recoverable sequential-attempt cleanup.
3. Keep `authUrl` in Rust, pass only a parsed approved URL to `UrlOpener`, and preserve `loginId`
   internally when cancellation is not confirmed so a later retry can clean it up.
4. Run focused and full desktop Rust checks, review size, and stop for user approval before commit,
   push or PR creation. Suggested commit: `feat(desktop): start ChatGPT browser login safely`.

## A1.2a-2a2b — Lifecycle Concurrency (future branch after A1.2a-2a2a merges)

## Task 4B2b: Harden browser login concurrency

**Files:**

- Modify: `apps/desktop/src-tauri/src/account/service.rs`
- Modify: `apps/desktop/src-tauri/src/account/service_tests.rs`

### Steps

1. Write failing tests for serialized concurrent starts, read/start and reconnect/start races, and
   defensive device-response cleanup.
2. Invalidate stale reads when installing status and guard every late result with its connection
   revision. Keep concurrent browser starts behind one serialized login-operation boundary.
3. Run focused and full desktop Rust checks, review size, and stop for user approval before commit,
   push or PR creation. Suggested commit: `feat(desktop): harden browser login concurrency`.

## A1.2a-2a3 — Device Code & Switching (future branch after A1.2a-2a2b merges)

## Task 4B3: Add device-code login and switching

**Files:**

- Modify: `apps/desktop/src-tauri/src/account/service.rs`
- Modify: `apps/desktop/src-tauri/src/account/service_tests.rs`

### Steps

1. Write failing tests for fixed device params, browser/device switching, controlled verification
   opening, failed-open retry recovery and temporary-value cleanup.
2. Extend the active attempt with device verification URL and user code; never expose browser URLs or
   login IDs. A successful retry after an opener error must restore the stored device-pending status.
3. Run focused and full desktop Rust checks, review size, and stop for user approval before commit,
   push or PR creation. Suggested commit: `feat(desktop): add ChatGPT device-code login`.

## A1.2a-2b — Completion & Account Actions (future branch after A1.2a-2a3 merges)

## Task 4B4: Complete the managed login lifecycle

**Files:**

- Modify: `apps/desktop/src-tauri/src/account/service.rs`
- Modify: `apps/desktop/src-tauri/src/account/service_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/connection.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/connection_request_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`

### Steps

1. With a manual task spawner, write failing tests for matching/stale completion notifications,
   cancel cleanup, parameterless logout and logout failure preserving signed-in state.
2. Promote `NotificationObserver` to crate-internal visibility without making the connection module
   public. Define a documented `TaskSpawner` trait with a narrow object-safe method.
3. Notification callbacks must return promptly and schedule deduplicated background `account/read`;
   they must never synchronously wait for an RPC on the App Server reader thread. Ignore stale login
   IDs.
4. Re-read backend truth after successful completion, cancel and logout. Clear temporary values on
   terminal paths; preserve signed-in state when logout fails. Never log URL, login ID, code or raw
   payloads.
5. Run focused and full desktop Rust checks, review size, and stop for user approval before commit,
   push or PR creation. Suggested commit: `feat(desktop): complete ChatGPT login lifecycle`.

## A1.2b — Account Bridge (future branch after A1.2a merges)

## Task 5: Expose narrow Tauri commands

**Files:**

- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/account/service.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/process.rs`
- Verify unchanged: `apps/desktop/src-tauri/capabilities/default.json`

### Steps

1. Add failing command-facing service tests for repeated start/cancel/logout and disconnected state.
2. Register only `get_account_status`, `start_chatgpt_login`, `start_device_code_login`,
   `cancel_account_login`, `logout_account`, and `open_device_verification`.
3. Use `spawn_blocking` for RPC waits. Return only `AccountStatus`; validate current backend state.
4. Emit normalized `account-status-changed` to `main`. On disconnect clear the active attempt before
   emitting an error. Do not emit raw notifications.
5. Confirm capability JSON still has no shell/open/spawn/execute permission. Run Rust checks, wait
   for approval, then commit `feat(desktop): expose account login commands`.

## Task 6: Add React account bridge and hook

**Files:**

- Create: `apps/desktop/src/types/account.ts`
- Create: `apps/desktop/src/lib/accountBridge.ts`
- Create: `apps/desktop/src/lib/accountBridge.test.ts`
- Create: `apps/desktop/src/hooks/useAccountStatus.ts`
- Create: `apps/desktop/src/hooks/useAccountStatus.test.tsx`

### Steps

1. Write failing bridge tests showing each export invokes one fixed command, passes no URL/method,
   listens only to the fixed event, and cleans up.
2. Implement typed functions for get/start browser/start device/cancel/logout/open device/listen; do
   not export a generic invoke helper.
3. Write failing Hook tests: no request before runtime connected, subscribe before initial read,
   new event beats stale read, reconnect rereads, actions deduplicate, unmount cleans listener, and
   rejected calls become the safe error.
4. Implement the Hook and run focused tests, `corepack pnpm test`, `corepack pnpm build`, and
   `corepack pnpm format`.
5. Wait for approval, then commit `feat(desktop): bridge account login state`.

## A1.2c — Account UI (future branch after A1.2b merges)

## Task 7: Build the accessible account UI

**Files:**

- Create: `apps/desktop/src/components/AccountAccessCard/AccountAccessCard.tsx`
- Create: `apps/desktop/src/components/AccountAccessCard/AccountAccessCard.module.css`
- Create: `apps/desktop/src/components/AccountAccessCard/AccountAccessCard.test.tsx`
- Modify: `apps/desktop/src/app/{App.tsx,App.module.css,App.test.tsx}`
- Modify: `apps/desktop/src/content/zh-CN.ts`
- Modify: `apps/desktop/src/components/AppShell/AppShell.tsx`

### Steps

1. Write failing semantic tests for checking, signed out, browser pending, device pending, signed in,
   error, runtime unavailable, copy/open/cancel, and logout confirmation.
2. For logout confirmation test dialog name, cancel-first focus, Escape and focus restoration.
3. Implement using existing tokens/CSS Modules and centralized Chinese copy. Add no router, UI/icon
   library, remote font or decorative animation.
4. Keep overview/service card and add the account card below it. Update stage copy without implying
   chat exists.
5. Run all frontend tests/build/format, review deterministic states, wait for approval, then commit
   `feat(desktop): add ChatGPT login experience`.

## Task 8: Verify and deliver

**Files:**

- Follow: `rivloom-docs/plans/2026-08-24-desktop-account-login-verification-plan.md`
- Create result: `rivloom-docs/plans/2026-08-24-desktop-account-login-verification.md`
- Modify only files needed for verified defects.

### Steps

1. From `apps/desktop`, run `just fmt`, `just test`, `just check`, `just test-rust`, and
   `just check-rust`; from `codex-rs`, run required `just fmt`.
2. Complete deterministic visual and fake-sidecar native QA from the verification plan.
3. Explain real authentication effects and wait for explicit approval before browser or device-code
   login. The user completes official login pages; never automate or inspect credentials.
4. Record commands, counts, screenshots, native results, known limitations and proof of no model
   calls in the result document.
5. Check no unplanned `codex-rs` changes or sensitive/generated files, then review total diff.
6. Wait for approval before the verification commit, push, PR or merge. Suggested verification
   commit: `test(desktop): verify account login lifecycle`.
