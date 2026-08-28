# Rivloom Desktop Account Login Verification

> 2026-08-28 update: device-code verification in this historical record is superseded by the [browser-only login decision](2026-08-28-desktop-browser-only-login-design.md).

- Verification date: 2026-08-27
- Branch: `codex/desktop-account-verification-a12c`
- Base: `3b0ac72d2b` (`main`, merged PR #19)
- Scope: A1 account protocol, desktop bridge, login UI, fake native lifecycle, and security review

## Result

Automated, visual, fake-native, and real browser-login verification passed. Three defects
were found and fixed: long unbroken account values could be clipped, the device verification address
wrapped poorly at the minimum window width, and successful browser login navigated onward to
ChatGPT. Account details now allow emergency wrapping, the verification address uses a smaller
readable size, and Rivloom requests App Server's local login-success page.

After separate explicit authorization, real logout/relogin revalidation confirmed the local browser
success page, signed-in UI convergence, and signed-in recovery after a full application restart. The
device-code flow remains pending because it requires separate, explicit user authorization immediately
before it starts. No real credential, OAuth URL, login ID, device code, account file, conversation
request, turn request, or model request was read or recorded during this verification.

## Automated checks

Commands were run from `apps/desktop` unless otherwise noted.

| Check                                                       | Result                                                                                                                                                 |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `just test`                                                 | Passed: 8 frontend test files, 40 tests                                                                                                                |
| `just check`                                                | Passed: 40 tests, TypeScript build, and Vite production build with 34 transformed modules                                                              |
| `just test-rust`                                            | Passed: 111 Rust tests                                                                                                                                 |
| `just check-rust`                                           | Passed                                                                                                                                                 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | Passed                                                                                                                                                 |
| Targeted Prettier check for the changed CSS module          | Passed                                                                                                                                                 |
| Repository formatter (`scripts/format.py`, from `codex-rs`) | Write mode completed with bundled Python; final check remains blocked by the checkout-wide CRLF mismatch in the root justfile and Bazel/Starlark files |
| `git diff --check`                                          | Passed; Git emitted only the expected Windows line-ending notice                                                                                       |

The first `just fmt` attempt could not start the Python configured first on the host `PATH`. Running
the same repository formatter in write mode with Codex's bundled Python completed outside the
restricted sandbox. It exposed the repository's existing `core.autocrlf=true` mismatch by touching
173 unrelated Bazel/Starlark files; those formatter-only changes were restored exactly. A final
read-only run consequently reports the same checkout-wide line-ending mismatch. The intended Rust,
CSS, and Markdown files pass their scoped formatters.

Tool versions used for the final checks:

- pnpm 10.34.5
- Node.js 24.19.0
- Rust 1.98.0
- just 1.58.0
- Vite 8.2.1
- Vitest 4.1.10

## Visual verification

Deterministic Tauri IPC mocks rendered the account UI at the default 1180 x 760 viewport and the
minimum 960 x 640 viewport. Temporary visual scaffolding was removed after the run.

| State                               | 1180 x 760                                | 960 x 640                                      |
| ----------------------------------- | ----------------------------------------- | ---------------------------------------------- |
| Checking                            | Passed                                    | Passed                                         |
| Signed out                          | Passed                                    | Passed                                         |
| Browser pending                     | Passed                                    | Passed                                         |
| Device-code pending                 | Passed                                    | Passed after the verification-address size fix |
| Signed in                           | Passed                                    | Passed                                         |
| Signed in with long unbroken values | Passed after emergency wrapping was added | Passed after emergency wrapping was added      |
| Retryable account error             | Passed                                    | Passed                                         |
| Runtime unavailable                 | Passed                                    | Passed                                         |
| Logout confirmation                 | Passed                                    | Passed                                         |

For both viewports:

- The document dimensions matched the viewport and had no document-level horizontal overflow.
- The bottom status bar ended at the viewport bottom while the main region scrolled internally.
- State meaning remained available in text and did not rely on color alone.
- Device-code values and long account details stayed inside the detail panel and remained readable.
- Reduced-motion mode disabled nonessential transitions.
- The logout dialog initially focused `暂不退出`; Tab and Shift+Tab stayed inside the dialog.
- Escape closed the dialog and restored focus to `退出账号`.
- A fresh post-fix tab reported no console warnings or errors.

Screenshots were written only to `%TEMP%\rivloom-account-login-screenshots`. They are not tracked by
Git and contain deterministic fake data only.

## Fake native verification

A temporary Rust JSONL App Server implemented only initialization and account methods. It was
compiled into `%TEMP%`, copied through the production sidecar preparation path, exercised through a
native Tauri window and WebView2 CDP connection, then deleted. The real App Server sidecar was
restored afterward.

1. Native initialization connected and the first `account/read` rendered signed out.
2. Starting device-code login rendered the pending state using fake values only.
3. Cancel returned to signed out. A later stale completion notification did not restore the canceled
   attempt.
4. Completion followed by account update converged to signed in after `account/read`.
5. Account update followed by completion also converged to signed in after `account/read`.
6. Logout displayed the confirmation dialog; confirming logout reread the account and rendered
   signed out.
7. Terminating the fake sidecar changed the runtime to a retryable connection error and disabled
   account actions.
8. Manual retry started a new sidecar connection, initialized it, reread the account, and rendered
   signed out.
9. Sending the standard Windows close event to the Rivloom window exited both `rivloom-desktop` and
   its fake `codex-app-server` child; neither process remained.

The native browser opener was deliberately not invoked: the production opener would launch an
external browser, which belongs to the separately authorized real-login phase. Official URL parsing
and opener behavior remain covered by the Rust tests.

The fake method log contained 28 entries. Its unique methods were limited to initialization,
`account/read`, account login start/cancel, account logout, and local test-control markers. It
contained zero `thread/start`, `turn/start`, response, conversation, or model methods.

After fake verification, the prepared and direct-run sidecar copies both matched the real source
binary:

- Size: 243,458,560 bytes
- SHA-256: `4F57C510209BE79AF617FF261A7293F71AD5B9D66411386C3D9DCC5A2D5C97FD`

All temporary fake source, executable, control, method-log, and CDP-driver files were removed.

## Initial real browser-login verification

After explicit authorization, Rivloom started with the restored real App Server and reported signed
out. Starting browser login opened the official authorization flow and placed Rivloom in browser
pending. The user completed authorization, Rivloom received the completion/update notifications,
reread the account, and rendered signed in. No email or OAuth data was captured in the verification
output.

The browser then navigated onward to ChatGPT. Investigation found that Rivloom explicitly sent:

- `useHostedLoginSuccessPage: true`
- `appBrand: "chatgpt"`

App Server correctly interpreted those fields as a request for its hosted ChatGPT-branded success
destination. The approved fix changes the request to `useHostedLoginSuccessPage: false` and omits
`appBrand`, preserving the existing OAuth, callback, notification, and account-refresh behavior while
ending successful login on App Server's local confirmation page.

The request-contract test was changed first. Before the production fix, the Rust suite produced the
expected focused failure: 100 passed and 11 account request assertions failed only on the hosted-page
fields. After the minimal production change, all 111 Rust tests passed, `just check-rust` passed, and
the frontend/build check again passed 8 files and 40 tests with 34 transformed production modules.

Design and implementation details are recorded in:

- `2026-08-27-desktop-account-local-login-success-page-design.md`
- `2026-08-27-desktop-account-local-login-success-page-plan.md`

## Real local success-page revalidation

After separate explicit authorization, the updated native application restored the existing signed-in
state and then exercised a real logout and browser login without recording account identifiers or
authorization data.

1. Confirming logout invoked App Server's normal logout path, which attempts server-side token
   revocation before clearing Rivloom's local authentication storage and account cache. It did not log
   the user out of the browser's ChatGPT session. Rivloom reread the account and rendered signed out.
2. Starting browser login opened the official authorization flow. After the user completed it,
   Rivloom received the normal completion/update events, reread the account, and rendered signed in.
3. The user confirmed that the browser stopped on App Server's local “login successful, this page can
   be closed” page and did not navigate onward to ChatGPT.
4. Sending the standard Windows close event exited Rivloom and its real App Server child with no
   sidecar process left behind.
5. Starting Rivloom again restored the signed-in state. A boolean-only native UI check reported the
   local core connected, signed in, not signed out, and no account-status error.

The revalidation did not read or output the authorization URL, login ID, token, email address, or
account file, and it did not call thread, turn, conversation, response, or model methods.

## Security and repository review

- React invokes exactly six account commands: status, browser login, device-code login, cancel,
  logout, and open-device-verification.
- `open_device_verification` accepts no URL from React. Rust opens only the validated URL stored in
  the active device-code attempt.
- The main-window capability contains only default window and event permissions. It contains no
  shell open, spawn, or execute permission.
- Browser and device verification URLs must use HTTPS, standard port 443, no user information, and
  an `openai.com` or `chatgpt.com` host or subdomain.
- The browser-login response has a redacted custom `Debug` implementation; account code does not
  log login IDs, URLs, codes, or raw account responses.
- JSONL lines are capped at 4 MiB, connection pending requests at 64, early completion tracking at
  8 IDs and 4 KiB, and diagnostics at 512 characters.
- Disconnect, cancellation, logout, retry, and application close clear pending requests and active
  attempts and terminate the sidecar process.
- `git diff 812c27ffa9 -- codex-rs` is empty.
- The working diff contains no binary, `target`, `dist`, screenshot, log, account-home, or credential
  artifact. Ignored local build outputs remain outside Git.
- `LICENSE` remains Apache-2.0 and no dependency or asset was added by this verification.

## Known limitations and pending work

- Browser-login cancellation, device-code login, device-code copy/open behavior, device-code
  cancellation, and final real-account logout still require explicit user participation and
  authorization.
- The native fake run covered Windows only.
- Full `corepack pnpm format` remains noisy on this Windows checkout because tracked LF files are
  checked out under `core.autocrlf=true`; the changed CSS file itself passes Prettier.
- Temporary screenshots are not durable repository artifacts and may be removed by system cleanup.
