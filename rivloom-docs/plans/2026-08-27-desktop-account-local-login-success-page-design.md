# Rivloom Desktop Local Login Success Page Design

- Status: approved
- Date: 2026-08-27
- Branch: `codex/desktop-account-verification-a12c`

## Context

The real browser-login flow completes successfully and Rivloom receives the signed-in account
state. After the OAuth callback, however, the browser navigates to ChatGPT. This is caused by
Rivloom explicitly requesting App Server's hosted login-success page with the ChatGPT brand.

The hosted destination is useful for Codex or ChatGPT clients that want a branded web handoff, but
it is surprising for a standalone Rivloom desktop flow. The browser has already finished its job at
that point, so opening another product makes the completion state ambiguous.

## Decision

Rivloom will request App Server's local login-success page for browser login by sending
`useHostedLoginSuccessPage: false` and omitting `appBrand`.

App Server already owns the callback listener and local success response. No Rivloom web server,
new route, upstream `codex-rs` change, dependency, capability, or external service is required.

## User flow

1. Rivloom starts browser login and opens the validated official authorization address.
2. The user completes ChatGPT authorization.
3. App Server handles the local callback, persists the account, and serves its local success page.
4. The browser tells the user that login succeeded and the page can be closed; it does not navigate
   to ChatGPT.
5. Rivloom receives the existing completion/update notifications, rereads `account/read`, and shows
   the signed-in state.

Organization-setup redirects remain owned by App Server. The local success-page preference applies
only when App Server considers the authorization flow complete.

## Alternatives considered

- Keep the hosted page but use the Codex brand: rejected because it still hands the user to another
  product instead of completing the Rivloom flow locally.
- Build a Rivloom-hosted success page: deferred because it adds hosting, branding, availability, and
  privacy responsibilities without improving the current milestone.

## Verification

- A Rust request assertion must require `useHostedLoginSuccessPage: false` and no `appBrand`.
- The full desktop Rust tests, check, frontend tests, build, formatting, and repository diff checks
  must remain green.
- A separately authorized real logout/login run must confirm the browser stays on the local success
  page while Rivloom reaches signed in.
