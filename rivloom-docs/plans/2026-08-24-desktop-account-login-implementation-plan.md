# Rivloom Desktop Account Login Implementation Plan

Plan status: validated design, awaiting implementation approval.

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

- Work only in `C:\project\opencohive\.worktrees\desktop-account-login` on
  `codex/desktop-account-login` at or after `584ba0a7c4`.
- Follow `2026-08-24-desktop-account-login-design.md` and the repository `AGENTS.md`.
- Do not modify `codex-rs`, App Server protocol, `CODEX_SANDBOX_*`, or upstream docs.
- Do not create a thread, send a turn, call a model, inspect credentials, or log sensitive values.
- Keep React without generic App Server, URL, shell, executable, path or environment access.
- Do not push, create a PR, merge, or run the complete upstream test suite without approval.

## Delivery checkpoints

- **A1.1:** Tasks 1–3, protocol foundation. Stop for diff/size review.
- **A1.2:** Tasks 4–8, account lifecycle and UI.
- If non-mechanical changes approach 800 lines, propose a two-PR split at A1.1 and wait for approval.

## Task 1: Add bounded wire parsing

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

## Task 2: Route requests and notifications

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

## Task 3: Keep the sidecar connection alive

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

## Task 4: Implement account state and service

**Files:**

- Create: `apps/desktop/src-tauri/src/account/{mod,service,types}.rs`
- Create: `apps/desktop/src-tauri/src/account/{service,types}_tests.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

### Steps

1. Write failing deep-equality/serialization tests for all six account states. Assert camelCase and
   absence of Token, login ID, raw error, environment and path fields.
2. Implement the approved Rust enum; unsupported account types map to a safe non-retryable error.
3. With fake connection/opener, write failing service tests for auth-required null, auth-not-required
   null, ChatGPT/unsupported accounts, nullable email, browser/device starts, official HTTPS URL
   checks, prior-attempt cancellation, matching/stale notifications, terminal cleanup, and logout
   failure preserving signed-in.
4. Implement `AccountService`. Use `tauri::Url` and existing `tauri-plugin-shell`; do not add WebView
   permission or an opener dependency. Never log URL, login ID, code or raw response.
5. Run `just test-rust`, `just check-rust`, `just fmt`; wait for approval, then commit
   `feat(desktop): manage ChatGPT account state`.

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
