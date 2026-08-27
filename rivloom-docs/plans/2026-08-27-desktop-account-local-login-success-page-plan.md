# Desktop Account Local Login Success Page Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Finish successful Rivloom browser login on App Server's local confirmation page instead of navigating to ChatGPT.

**Architecture:** Keep the existing OAuth, URL validation, callback server, notification handling, and account refresh flow unchanged. Change only Rivloom's `account/login/start` request preference from the hosted ChatGPT success page to App Server's local success page, and lock the wire shape with the existing request helper used throughout account-service tests.

**Tech Stack:** Rust 2024, Tauri 2, serde_json, repository `just` workflows

**Status (2026-08-27):** Tasks 1–4 are complete. The real flow confirmed the local success page,
signed-in UI convergence, and signed-in recovery after restarting Rivloom. No Git publication action
has been authorized or performed.

---

### Task 1: Lock the local success-page request contract

**Files:**

- Modify: `apps/desktop/src-tauri/src/account/service_tests.rs`
- Test: `apps/desktop/src-tauri/src/account/service_tests.rs`

**Step 1: Change the expected browser-login request**

Update `browser_start_request()` to require this complete request object:

```rust
request(
    "account/login/start",
    json!({
        "type": "chatgpt",
        "useHostedLoginSuccessPage": false,
    }),
)
```

The absence of `appBrand` is intentional because branding applies only to the hosted page.

**Step 2: Run the Rust suite and verify the test fails**

Run from `apps/desktop`:

```text
just test-rust
```

Expected: at least one account-service request assertion fails because production still sends
`useHostedLoginSuccessPage: true` and `appBrand: "chatgpt"`.

### Task 2: Request App Server's local success page

**Files:**

- Modify: `apps/desktop/src-tauri/src/account/service.rs`
- Test: `apps/desktop/src-tauri/src/account/service_tests.rs`

**Step 1: Make the minimal production change**

Change the browser-login request to:

```rust
json!({
    "type": "chatgpt",
    "useHostedLoginSuccessPage": false,
})
```

Do not change URL validation, the opener, login-attempt state, notifications, refresh behavior, or
`codex-rs`.

**Step 2: Run the Rust suite and verify it passes**

Run:

```text
just test-rust
```

Expected: 111 Rust tests pass.

### Task 3: Verify the desktop change

**Files:**

- Modify: `rivloom-docs/plans/2026-08-24-desktop-account-login-verification.md`

**Step 1: Run formatters and static checks**

Run from `apps/desktop`:

```text
cargo fmt --manifest-path src-tauri/Cargo.toml
just check-rust
just check
```

Run targeted Prettier on the new Markdown files and changed CSS. Run the repository formatter with
the bundled Python runtime if the host's first `python.exe` remains invalid.

**Step 2: Review repository safety**

Run from the worktree root:

```text
git diff --check
git status --short
git diff 812c27ffa9 -- codex-rs
```

Expected: no `codex-rs` difference and no binary, credential, account-home, screenshot, log,
`target`, or `dist` artifact in the tracked diff.

**Step 3: Update the verification record**

Record the diagnosed hosted redirect, the approved local-page decision, exact checks, and the fact
that real browser revalidation remains gated on a separate logout/relogin authorization.

### Task 4: Real browser revalidation

**Files:**

- Modify after success: `rivloom-docs/plans/2026-08-24-desktop-account-login-verification.md`

**Step 1: Obtain explicit authorization**

Explain that this step logs the current Rivloom account out and starts a new official ChatGPT
browser login. Do not proceed on implementation approval alone.

**Step 2: Run the real flow**

Verify without recording the full OAuth URL, login ID, token, device code, account file, or email:

1. Confirm logout in Rivloom.
2. Start browser login.
3. User completes authorization.
4. Confirm the browser stays on App Server's local success page and does not open ChatGPT.
5. Confirm Rivloom reaches signed in.
6. Restart Rivloom and confirm signed-in recovery.

**Step 3: Commit only after approval**

After reporting the final diff and checks, obtain separate authorization before staging, committing,
pushing, opening a pull request, or merging.
