# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

<!-- Inferred, not asked: a Tauri v2 desktop app whose UI is React in a webview (Linux first; macOS and Windows are targets). It follows desktop conventions, not a website's. -->

## Users

Developers who run several coding agents (Claude Code, Codex, OpenCode and other CLI agents) at the same time on the same codebase. They keep roughly 4 to 8 agents and worktrees active at once and switch between them all day. They work in a terminal-centric flow and care about speed. Inferred from the repository and the owner's environment: Linux with a tiling window manager, where the app often gets a half-screen, tall and narrow window (observed 950 x 1028 px).

The product is meant to be commercial, so audiences beyond the author are in scope, though no other segment has been defined yet.

## Product Purpose

agent-ade is an Agentic Development Environment: a desktop app that runs many coding agents in parallel, each isolated in its own git worktree, with terminals, agent status and code review in one place. Success means a developer can start, watch, steer and finish several agent tasks without juggling branches, stashes or terminal windows, and always knows which agent needs them.

## Positioning

Reproduce the core of Orca (worktree-per-task, agent terminals, status, review) and be much faster and lighter than it. The mechanism is a native Rust/Tauri backend that owns processes and state, with a thin React interface. Orca is an Electron app. No benchmark exists yet, so speed and weight are goals, never claims.

## Operating Context

- Each task is a git worktree with its own branch, created from a base ref. Worktrees live next to the repo in `<repo>-worktrees/`.
- Each worktree can hold several terminal sessions. An agent command is typed into a login shell, so the tab remains a shell after the agent exits.
- Agents run for minutes to hours in the background. The user glances across worktrees to see which agent is working, idle, or waiting for them.
- Repo state comes from Git, which stays the source of truth; the app only remembers which repos are open and what each worktree was branched from.

## Capabilities and Constraints

Built so far (backend): register repos, list/create/remove worktrees, PTY terminals with scrollback and typed events, SQLite persistence.

First interface scope: open a repo, list worktrees, create and remove a worktree, open terminals (shell or agent command) per worktree, see basic agent state (working or idle).

Later phases, not part of the minimal interface: diff review and commit/push, quick open and jump palette, GitHub/GitLab/Linear/Jira, embedded browser, orchestration, remote/SSH, mobile.

Constraints:

- Terminals die when the app closes in this phase; a separate PTY daemon is planned for a later phase.
- Must work on Linux, macOS and Windows; shortcuts use Cmd on macOS and Ctrl elsewhere.
- Frontend is React with shadcn/ui and Tailwind; the design uses the existing token system in `src/index.css`.
- The UI language, product name and visual identity are undecided.

## Brand Commitments

- **Visual direction: the Orca family, executed at full craft.** The owner prefers the established direction of Orca (dark neutral chrome, three zones: project/worktree tree, terminal tabs, source-control panel; dense small type, hairline dividers, status dots and agent glyphs) and wants to adjust it to their own taste rather than invent a new visual world. This was chosen in place of the rolled directions (Wire Desk, Timing Tower, Line Diagram, Flight Strip Rack). Structure, density and conventions follow the reference; Orca's icons, marks and exact layout are not copied, because agent-ade is commercial.
- **Craft bar: Orca, Linear and Raycast.** Lists must feel as fast, dense and consistent as Linear, with keyboard-first command surfaces as in Raycast.
- **Baseline first, differences later.** The first interface reproduces the basics of the Orca family well; deliberate departures from it (e.g. right panel closed by default, stronger "needs you" state, leaner sidebar, quieter status bar) are deferred until the basics work and are not part of the minimal interface.
- "agent-ade" is a working name.

## Evidence on Hand

No logos, benchmarks, customers or testimonials exist. Do not fabricate performance numbers, comparisons with Orca, or social proof.

## Product Principles

- **Status over chrome.** The central question is "which agent needs me?", so agent state is the most legible thing on screen.
- **Terminal first.** Terminals are where the work happens; the interface frames them and never competes with them.
- **Instant and light.** Speed and low weight are the reason to exist. Nothing blocks the interface, and every interaction responds immediately.
- **Git is the truth.** The app reflects real repository state and never invents its own.
- **Keyboard-driven.** Frequent actions have shortcuts and a command surface; the mouse is optional.

## Accessibility & Inclusion

No product-specific standard established. Open decision: target at least WCAG AA contrast and full keyboard operation, since the product is commercial.
