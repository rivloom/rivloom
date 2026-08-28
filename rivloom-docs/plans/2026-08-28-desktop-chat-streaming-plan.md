# Rivloom Desktop A3 Chat and Streaming Implementation Plan

> **执行说明：** 按阶段逐项实施；每个阶段先写失败测试，再实现，并在进入下一阶段前复核真实 diff 和验证结果。

**Goal:** Deliver bounded existing-thread recovery and a safe read-only text chat/streaming loop for Rivloom Desktop.

**Architecture:** Stabilize a byte-bounded App Server summary-history path first. Then keep chat protocol ownership in Rust behind one fixed notification router, expose only normalized bounded DTOs to React, and reconcile unknown turns after reconnect instead of resending.

**Tech Stack:** Rust, App Server v2 JSON-RPC/JSONL, Tauri 2, React 19, TypeScript, Vitest, Testing Library, CSS Modules.

---

**Status:** Planned. Execute stages in order. Each stage or lettered substage is a separate PR based on the latest merged `origin/main`.

## Guardrails for every stage

- Start only from the current `origin/main`; never reuse the stale main checkout.
- Preserve all existing worktrees, branches and dirty history. New branches use `codex/`.
- Keep total changed lines below 800 and complex logic below 500. If the actual diff exceeds either limit, stop and split at the nearest tested boundary before committing.
- Do not read rollout files from Rivloom, enlarge the 4 MiB decoder, enable unrelated experimental APIs, call a real model, or add write/network permission.
- Test first. Rust test modules are sibling `*_tests.rs` files. UI changes require snapshots.
- App Server API changes update `app-server/README.md`, schemas and public JSON-RPC integration tests.
- Run `just fmt` after any `codex-rs` code change. Use `just test`, never direct `cargo test`, inside `codex-rs`.

## Stage A3.0 — Stable byte-bounded history protocol

### Task 1: Add failing public protocol tests

**Files:**

- Modify: `codex-rs/app-server/tests/suite/v2/experimental_api.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/thread_read.rs`

**Steps:**

1. Add a non-experimental initialization test proving metadata-only resume and summary turns listing are accepted.
2. Add 0/1/20-turn fixtures plus one user text, agent text and error larger than 4 MiB.
3. Request `limit: 20`, summary and a 3 MiB result budget. Assert serialized `result` is at most the requested budget, every cursor advances, and truncated turn IDs are explicit.
4. Cover legacy and paginated rollout plus a running turn. Run the named tests and confirm they fail on the current experimental/unbounded behavior.

### Task 2: Implement the minimal protocol contract

**Files:**

- Modify: `codex-rs/app-server-protocol/src/protocol/v2/thread.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`
- Create: `codex-rs/app-server/src/request_processors/bounded_turn_history.rs`
- Create: `codex-rs/app-server/src/request_processors/bounded_turn_history_tests.rs`
- Modify: `codex-rs/app-server/src/request_processors/thread_processor.rs`
- Modify: `codex-rs/app-server/src/request_processors/mod.rs`
- Modify: `codex-rs/app-server/README.md`

**Steps:**

1. Stabilize `excludeTurns` and summary/notLoaded `thread/turns/list`; keep full/items/initial-page paths out of Rivloom usage.
2. Add nullable request `maxBytes` and response `truncatedTurnIds` with aligned camelCase Rust/TS names.
3. Implement a private bounded summary projector. Truncate on UTF-8 boundaries, cap every retained string, serialize incrementally, and stop before the next turn would exceed the result budget.
4. Guarantee progress for a first oversized turn by returning a bounded summary and marking its turn ID; never return the same forward cursor.
5. Run `just write-app-server-schema` and the experimental variant only if affected. Inspect generated changes and split mechanical fixtures if the PR would exceed 800 reviewable lines.
6. Run `just fmt`, `just test -p codex-app-server-protocol`, then the named `codex-app-server` tests. Commit as `feat(app-server): bound thread summary history`.

## Stage A3.1 — Fixed backend connection router

### Task 3: Route connection lifecycle and notifications to account and chat services

**Files:**

- Create: `apps/desktop/src-tauri/src/app_server/connection_router.rs`
- Create: `apps/desktop/src-tauri/src/app_server/connection_router_tests.rs`
- Create: `apps/desktop/src-tauri/src/chat/mod.rs`
- Create: `apps/desktop/src-tauri/src/chat/service.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/state.rs`
- Modify: `apps/desktop/src-tauri/src/account/service/login_completion.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Steps:**

1. Write tests for account-only, chat-only and unknown notifications plus connected/disconnected delivery. Assert the source connection identity is preserved and payloads are not logged.
2. Add the smallest inert `ChatService` shell so the router has a real, fixed destination before chat session state exists; it must not emit events or retain raw payloads yet.
3. Implement one `ConnectionRouter` holding `Arc<AccountService>` and `Arc<ChatService>` sinks; install the same router once as both supervisor connection observer and notification observer.
4. Preserve account login completion behavior and keep stale-connection rejection inside each service, where the active identity and revision are authoritative.
5. Run desktop Rust tests and check the diff. Commit as `refactor(desktop): route app-server events`.

## Stage A3.2 — Bounded chat contracts and history adapter

### Task 4: Normalize resume and history pages in Rust

**Files:**

- Modify: `apps/desktop/src-tauri/src/chat/mod.rs`
- Create: `apps/desktop/src-tauri/src/chat/types.rs`
- Create: `apps/desktop/src-tauri/src/chat/types_tests.rs`
- Create: `apps/desktop/src-tauri/src/chat/protocol.rs`
- Create: `apps/desktop/src-tauri/src/chat/protocol_tests.rs`

**Steps:**

1. Write parser tests for valid 0/1/20 pages, truncation flags, malformed IDs/cursors, overlong fields, wrong cwd and page/result caps.
2. Define fixed DTO unions for user, assistant, reasoning, command, generic tool and blocked file-change items. Do not store raw JSON.
3. Serialize only metadata-only resume and 20-turn summary requests with 3 MiB `maxBytes`; define the turn/start request builder here so every later caller is forced to use read-only sandboxing, network off and `approvalPolicy: "never"`.
4. Parse with per-field and aggregate limits before allocating React DTOs.
5. Run desktop Rust tests. Commit as `feat(desktop): add bounded chat protocol`.

## Stage A3.3 — Thread lifecycle and race isolation

### Task 5: Open, page, release and reconnect one chat session

**Files:**

- Create: `apps/desktop/src-tauri/src/chat/state.rs`
- Modify: `apps/desktop/src-tauri/src/chat/service.rs`
- Create: `apps/desktop/src-tauri/src/chat/service_tests.rs`
- Create: `apps/desktop/src-tauri/src/chat/lifecycle.rs`
- Create: `apps/desktop/src-tauri/src/chat/lifecycle_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/state.rs`

**Steps:**

1. Write fake-connection tests for open order, cwd verification, subscription buffering, older-page dedupe, cursor stall, switch, exit and best-effort unsubscribe.
2. Add tests for resume timeout followed by a late response, old connection events and reconnect while a turn is active.
3. Implement `lifecycleRevision` and match the full isolation key. Invalidate before any cleanup request.
4. Bound loading/reconcile buffers to 512 events or 2 MiB; overflow forces authoritative re-read.
5. Mark disconnect as `outcomeUnknown`; reconnect with a new revision and summary reconciliation.
6. Run desktop Rust tests. Commit as `feat(desktop): manage chat thread lifecycle`.

## Stage A3.4a — Turn/item reducer and bounded delta batches

### Task 6A: Reduce streaming events

**Files:**

- Create: `apps/desktop/src-tauri/src/chat/reducer.rs`
- Create: `apps/desktop/src-tauri/src/chat/reducer_tests.rs`
- Create: `apps/desktop/src-tauri/src/chat/delta_buffer.rs`
- Create: `apps/desktop/src-tauri/src/chat/delta_buffer_tests.rs`

**Steps:**

1. Test started-before-response, completion-without-start, delta-before-item, duplicate completion, late delta, failure, retryable error, interrupt and reconnect reconciliation.
2. Implement exhaustive session/turn/item transitions. Only `item/completed` and `turn/completed` seal their respective states.
3. Merge deltas in Rust and emit at most about 30 batches per second with aggregate caps.
4. Run desktop Rust tests. Commit as `feat(desktop): reduce streaming chat events`.

## Stage A3.4b — A4-safe server request boundary

### Task 6B: Reject side effects before they can reach React

**Files:**

- Modify: `apps/desktop/src-tauri/src/app_server/connection.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/connection_inbound_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/process.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/process_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/state.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/connection_router.rs`
- Create: `apps/desktop/src-tauri/src/chat/server_requests.rs`
- Create: `apps/desktop/src-tauri/src/chat/server_requests_tests.rs`

**Steps:**

1. Add a bounded synchronous server-request handler contract: request IDs remain inside the connection layer, while method and borrowed params are handled without retention and produce a typed result or bounded protocol error.
2. Install the existing fixed `ConnectionRouter` as the server-request handler for every current and future connection; route requests only to `ChatService`.
3. Test each known approval/input/permission/MCP request. Return typed decline/cancel where defined; use bounded `-32601` only for unsupported methods.
4. Assert request payloads, tool arguments and response bodies are never logged or emitted to React.
5. Run desktop Rust tests and check the diff. Commit as `feat(desktop): reject chat server requests`.

## Stage A3.5 — Drafts, sending and fixed Tauri commands

### Task 7: Persist drafts and prevent duplicate sends

**Files:**

- Create: `apps/desktop/src-tauri/src/chat/storage.rs`
- Create: `apps/desktop/src-tauri/src/chat/storage_tests.rs`
- Create: `apps/desktop/src-tauri/src/chat/commands.rs`
- Create: `apps/desktop/src-tauri/src/chat/commands_tests.rs`
- Modify: `apps/desktop/src-tauri/src/chat/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Steps:**

1. Test versioned load/save, corruption, atomic replacement, stale temp files, 20-draft/256 KiB caps and Windows path behavior.
2. Test 32 KiB input, one in-flight submission, pre-send failure draft restoration, timeout outcome unknown and no automatic resend.
3. Generate `clientUserMessageId` in Rust and retain a bounded submission record until reconciliation.
4. Expose fixed commands for open, older page, send, interrupt, save draft and release; accept project/thread IDs, never arbitrary cwd.
5. Run both normal desktop Rust tests and the Tauri command-feature tests. Commit as `feat(desktop): add chat commands and drafts`.

## Stage A3.6 — React bridge and deterministic state

### Task 8: Add typed events, bridge, reducer and hook

**Files:**

- Create: `apps/desktop/src/types/chat.ts`
- Create: `apps/desktop/src/lib/chatBridge.ts`
- Create: `apps/desktop/src/lib/chatBridge.test.ts`
- Create: `apps/desktop/src/hooks/chatReducer.ts`
- Create: `apps/desktop/src/hooks/chatReducer.test.ts`
- Create: `apps/desktop/src/hooks/useChatSession.ts`
- Create: `apps/desktop/src/hooks/useChatSession.test.tsx`

**Steps:**

1. Test exact Tauri command/event names, unlisten cleanup and rejection of malformed normalized events.
2. Test reducer ordering, duplicate batches, old revision, 200-turn/8 MiB eviction, truncated history and outcome unknown.
3. Implement one hook instance for the selected thread; cleanup releases the backend revision.
4. Keep all raw protocol names and JSON out of React types.
5. Run frontend tests and build. Commit as `feat(desktop): add chat frontend state`.

## Stage A3.7 — Transcript and composer UI

### Task 9: Render bounded chat and streaming controls

**Files:**

- Create: `apps/desktop/src/components/ChatTranscript/ChatTranscript.tsx`
- Create: `apps/desktop/src/components/ChatTranscript/ChatTranscript.module.css`
- Create: `apps/desktop/src/components/ChatTranscript/ChatTranscript.test.tsx`
- Create: `apps/desktop/src/components/ChatTranscript/__snapshots__/ChatTranscript.test.tsx.snap`
- Create: `apps/desktop/src/components/ChatComposer/ChatComposer.tsx`
- Create: `apps/desktop/src/components/ChatComposer/ChatComposer.module.css`
- Create: `apps/desktop/src/components/ChatComposer/ChatComposer.test.tsx`
- Modify: `apps/desktop/src/components/ProjectWorkspace/ProjectWorkspace.tsx`

**Steps:**

1. Snapshot empty, loading, long history, streaming, reasoning, tool, truncated, failed, interrupted, outcome-unknown and quota states.
2. Implement an accessible virtualized transcript with stable item keys and top-triggered older paging.
3. Implement multiline draft input, byte counter, send-once and interrupt controls. Never show approve or diff actions.
4. Verify 1180×760 and 960×640 with long Chinese text and long Windows/macOS/Linux path labels.
5. Run frontend tests and build. Commit as `feat(desktop): render streaming chat workspace`.

## Stage A3.8 — Recovery matrix and final verification

### Task 10: Prove the complete A3 boundary

**Files:**

- Create: `rivloom-docs/plans/2026-08-28-desktop-chat-streaming-verification.md`
- Update local excluded file: `rivloom-docs/CURRENT_STATUS.md`

**Steps:**

1. From `codex-rs`, run `just fmt`, scoped `just fix -p` for changed crates, protocol tests and the app-server public JSON-RPC suite required by A3. Ask before a complete workspace `just test`.
2. From `apps/desktop`, run `just fmt`, `just check`, `just test-rust` and `just check-rust`.
3. Run fake-sidecar scenarios for 4 MiB boundaries, late resume, old connection/project/thread/turn/item events, delta loss, reconnect and every server-request rejection. Assert zero real model endpoints.
4. Perform deterministic Windows smoke and visual checks; record CI coverage for macOS/Linux.
5. Audit every PR against 800/500 lines, raw JSON exposure, log payloads, permissions, experimental capability, rollout reads and JSONL limit changes.
6. Record actual commits, test counts, snapshots, known limitations and A4 as the only next product stage. Commit as `docs: record A3 chat verification`.

## Execution handoff

Begin with A3.0 only. Do not create desktop implementation branches until the bounded App Server protocol PR is merged and its exact wire schema is the new `origin/main`. After each stage, review the real diff and tests before creating the next worktree.
