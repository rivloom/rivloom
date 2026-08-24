# Rivloom Desktop Shell Verification

- Verification date: 2026-08-24
- Branch: `feat/desktop-shell`
- Scope: Windows desktop shell, typed runtime status bridge, and supervised App Server initialization

## Result

The first Rivloom Desktop shell passes its scoped automated, visual, and native lifecycle checks.
The verification found one layout defect: connected content could grow the document beyond the
window and push the bottom status bar off-screen. The shell now uses the viewport height so only the
main content region scrolls and the status bar remains visible.

No account, conversation, or model request was made, and no model quota was consumed.

## Automated checks

Commands were run from `apps/desktop` unless otherwise noted.

| Check | Result |
| --- | --- |
| `just test` | Passed: 5 frontend test files, 18 tests |
| `just check` | Passed: frontend tests, TypeScript build, and Vite production build |
| `just test-rust` | Passed: 15 Rust tests |
| `just check-rust` | Passed without Rust code warnings |
| `just fmt` | Passed after the verified layout fix |
| Repository formatter from `codex-rs` | Passed after the verified layout fix |

The Windows linker emitted its informational import-library message during Rust tests; it did not
affect the build or test result.

## Visual verification

Starting, connected, and error states were rendered with deterministic local Tauri IPC mocks at the
default and minimum content viewport sizes. The mock server and mock data were temporary and were
removed after verification.

| Viewport | State | Result |
| --- | --- | --- |
| 1180 × 760 | Starting | Passed: status card, text status, and bottom status bar visible |
| 1180 × 760 | Connected | Passed: details readable, long values ellipsized, main region scrolls internally |
| 1180 × 760 | Error | Passed: alert and enabled retry action visible |
| 960 × 640 | Starting | Passed: no horizontal overflow; main region scrolls internally |
| 960 × 640 | Connected | Passed: no horizontal overflow; fixed status bar remains visible |
| 960 × 640 | Error | Passed: recovery action reachable through internal scrolling and keyboard |

For all six states:

- The document dimensions matched the viewport dimensions exactly.
- There was no document-level horizontal overflow.
- The bottom status bar ended exactly at the viewport bottom.
- Chinese interface text and monospace runtime values rendered correctly.
- State meaning remained available as text and did not rely on color alone.
- The retry button was enabled in the retryable error state.
- Keyboard activation moved focus to the retry button and showed a visible 2 px focus outline.
- A clean post-fix browser session reported no console warnings or errors.

Temporary screenshots:

- `%TEMP%\rivloom-task7-screenshots\starting-1180x760.png`
- `%TEMP%\rivloom-task7-screenshots\connected-1180x760.png`
- `%TEMP%\rivloom-task7-screenshots\error-1180x760.png`
- `%TEMP%\rivloom-task7-screenshots\starting-960x640.png`
- `%TEMP%\rivloom-task7-screenshots\connected-960x640.png`
- `%TEMP%\rivloom-task7-screenshots\error-960x640.png`

These screenshots are verification artifacts and are not tracked by Git.

## Native lifecycle verification

The native Windows application was tested with the real App Server executable.

1. The built sidecar copy was temporarily renamed inside `src-tauri/target/debug`; the source build
   and prepared binary remained untouched.
2. Rivloom opened and transitioned `stopped → starting → error`.
3. The UI showed the approved generic error message and an enabled `重试连接` button. It did not
   expose the executable path or raw launch error.
4. The exact sidecar file was restored to its original path while Rivloom remained open.
5. The retry action was triggered through keyboard navigation.
6. The backend transitioned `error → starting → connected` and completed the real JSONL
   initialization handshake.
7. The connected response reported:
   - Rivloom version: `0.1.0-alpha.0`
   - Platform: `windows/windows`
   - Codex home: `%LOCALAPPDATA%\com.rivloom.desktop\codex-home`
8. Closing Rivloom transitioned the supervisor to `stopped`.
9. No `rivloom-desktop` or `codex-app-server` process remained, the sidecar was restored, and no
   temporary backup remained.

Native screenshots:

- `%TEMP%\rivloom-task7-screenshots\native-missing-sidecar-error.png`
- `%TEMP%\rivloom-task7-screenshots\native-retry-connected.png`

## Security and network observations

- React can invoke only the fixed `get_runtime_status` and `retry_app_server` commands.
- Runtime changes use one fixed `runtime-status-changed` event targeted to the `main` window.
- The webview capability contains no shell spawn or execute permission.
- Process paths, environment variables, and raw App Server JSONL are not exposed to React.
- The App Server attempted its automatic featured/curated plugin synchronization and received HTTP
  401/429 responses. This traffic was independent of the initialization handshake and did not call a
  model or consume model quota. A future network proxy must also cover this background App Server
  traffic.

## Known limitations

- This milestone exposes runtime status only; account, conversation, turn, approval, model, and proxy
  features are not implemented.
- Connected details require internal vertical scrolling at smaller heights. The status bar remains
  fixed and long runtime values use intentional ellipsis.
- Visual and native lifecycle verification in this record covers Windows only.
- Temporary screenshots are not durable repository artifacts and may be removed by system cleanup.
