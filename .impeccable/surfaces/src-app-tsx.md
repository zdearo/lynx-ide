---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: []
---

# Surface brief: main window (sidebar + terminal workspace)

## Scope

Mode: Operate. Surface: the whole minimal app window. Fidelity: production-ready basics, English UI strings kept in one place. Not included: source-control panel, status bar, global search, tasks, settings, theme switcher.

## Audience, job, task

Developer running 4-8 coding agents in parallel. Opens a repo, sees worktrees grouped by repo, creates an empty worktree, then opens a shell or agent terminal in it and switches between them. Window is half-screen (~950px) or full screen: the sidebar is fixed when wide and collapsible when narrow.

## Chosen direction

The Orca family at full craft, taken through the standing exit (category standard) at the user's request. Craft bar: Orca, Linear, Raycast. No deliberate departures yet. Do not copy Orca's icons, marks or exact layout.

## Direction contract

THESIS: A quiet dark workspace where the terminal is the product and the chrome only answers "which worktree, which agent". Refuses decoration, cards-in-cards and any chrome louder than the terminal.
OWN-WORLD: Neutral near-black surfaces from the existing shadcn tokens, 1px hairline dividers, small dense UI type (12-13px) with tabular numerals, one green for live state and one warm accent reserved for agent glyphs. Icons from one library (Lucide), one stroke weight.
STORY: The user sees at a glance which repos and worktrees exist and which have a running terminal, and reaches a running agent in a new worktree in three actions: open repo, new worktree, new terminal.
FIRST VIEWPORT: Left sidebar ~260px: header with "Open repository", a search field and the All | Active filter, then folders as group labels and folder rows flush left, repos one step in, each repo holding single-line worktree rows (state dot, branch, a small star right after the name on the main one, and a button that switches the worktree between its terminals and its team). Right: tab bar with terminal tabs and a "+" menu, terminal filling the rest; empty worktree shows a centered agent launcher.
FORM: Category standard (Orca family), chosen by the user over the rolled directions; roll key fe5e494c.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Unresolved

Deviations from Orca (right panel default, stronger needs-you state, leaner sidebar, quieter status bar) are deferred by the user.
