# Rivloom documentation

This directory contains product and engineering documentation created by the
Rivloom project. It is intentionally separate from the repository's `docs/`
directory, which is reserved for upstream Codex documentation.

## Current direction

- [Runtime Host 与协作闭环设计](plans/2026-08-30-runtime-host-collaboration-design.md)
  is the current authoritative product and architecture design.
- [Runtime Host Transition Implementation Plan](plans/2026-08-30-runtime-host-transition-plan.md)
  defines the active milestones and delivery gates.
- [R1/R2 Runtime Host Verification](plans/2026-08-30-runtime-host-r1-r2-verification.md)
  records the current local implementation, automated evidence, publication state, and remaining
  native smoke checks.
- [R1/R2 stacked PR queue](plans/2026-08-30-runtime-host-pr-stack.md) records the exact review order
  and separates uploaded heads from local-only branches.
- [2026-08-24 Rivloom Desktop 架构设计](plans/2026-08-24-rivloom-desktop-architecture-design.md)
  is retained as the historical A0–A2 baseline; its chat-first milestone order is superseded.

## Contents

- `plans/`: validated product and system designs.
- `adr/`: architecture decision records (ADRs).

## Accepted architecture decisions

- [ADR-0001: Tauri, React, and App Server sidecar](adr/0001-use-tauri-react-and-app-server-sidecar.md)
- [ADR-0002: Isolated Rivloom Codex home](adr/0002-isolate-rivloom-codex-home.md)
- [ADR-0003: Separate Rivloom product code from upstream Codex](adr/0003-separate-rivloom-code-from-upstream-codex.md)
- [ADR-0004: Stable cwd protocol for local projects](adr/0004-use-stable-cwd-for-local-projects.md)
- [ADR-0005: External Agent Runtimes](adr/0005-use-external-agent-runtimes.md)
- [ADR-0006: Separate Rivloom Identity from Runtime Auth](adr/0006-separate-rivloom-identity-from-runtime-auth.md)
