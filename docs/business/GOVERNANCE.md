# Governance — TUI-OP-HUB

How work is organized, decided and shipped.

## Roles

| Role | Responsibility |
|:---|:---|
| **Maintainer** (project owner) | Owns scope, merges to `dev`/`main`, cuts releases |
| **AI agents** | Implement stories per [`AGENTS.md`](../development/AGENTS.md); never leave work uncommitted or untested |
| **Contributors** | Propose changes via branches + conventional commits; keep docs truthful |

## Branching & commits

- Branches: `dev` (integration) → `main` (releases); feature branches off `dev`
- **Conventional commits** referencing story IDs:
  `feat(tui): working create/edit/delete forms (US-CMD-01, US-PROJ-01)`
- Types: `feat`, `fix`, `test`, `docs`, `refactor`, `chore` · Scope: module area
- Never commit: secrets, keys, `*.db` files, `target/`, `*.sync-conflict-*`

## Definition of done (for every change)

1. Story ID referenced in code comments and [`USER_STORIES.md`](../reference/USER_STORIES.md)
2. Unit tests in the touched module **and** a BDD scenario in `tests/bdd_scenarios.rs`
3. `cargo fmt` + `cargo clippy --all-targets` (0 errors) + `cargo test` + `cargo build` all green
4. Docs updated (README/ARCHITECTURE where behavior changed) + [`WORK.md`](../development/WORK.md) session note
5. Committed per the workflow above

## Decision process

1. **Scope** comes from user stories; new ideas get a story ID first (`bp.md` wins on conflicts)
2. **Architecture changes** (new deps, new modules, schema) are discussed in the story
   before implementation — see the rules in [`AGENTS.md`](../development/AGENTS.md)
3. **Quality gates are non-negotiable**: a red build or failing test blocks any merge

## Quality bar

- Secrets never in plaintext; SQL only in `repository/`; errors via `AppResult`
- Every behavior change ships with tests (unit + BDD) — no exceptions
