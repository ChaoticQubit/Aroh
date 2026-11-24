# Aroh agent guide

Aroh is a goal-based calendar. Goals become planned actions on a calendar; agents plan and replan them. Read `docs/decisions.md` before planning: it is the settled stack, product rules and data model.

## Local-only agent records

`docs/agent-decisions.md` (the decision log, Q1 onward) and `docs/backlog.md` are local-only: gitignored, never committed, kept on disk in the main checkout. Agents read them before planning. In a git worktree they are missing, so symlink them in first, from the worktree root:

```sh
main="$(git worktree list --porcelain | sed -n '1s/^worktree //p')"
ln -s "$main/docs/agent-decisions.md" docs/agent-decisions.md
ln -s "$main/docs/backlog.md" docs/backlog.md
```

The other process records (`docs/superpowers/`, `docs/gate/`, `docs/pods/`, `docs/screenshots/`, `docs/demo/`, `docs/open-source.md`, `e2e/pod*.js`, `e2e/pod*.sh`) are local-only the same way. Never `git add` them; `git add -A` is safe since they are ignored.

## Pod process

- One pod per feature. The pod lead is the project manager (PM): plans, keeps work aligned with the owner, asks the owner only real product decisions.
- The PM starts one engineer sub-agent. The engineer writes the tests first, then the code.
- No code reviews in the MVP. Only small tests for critical logic, no broad suites.
- Tests run in small containers with Kiri (`/usr/local/bin/kiri`, a Docker-compatible CLI: `kiri build`, `kiri run --rm`, `kiri compose`). Not OrbStack, not docker.
- MVP speed first.
- No code comments except docstrings directly above a declaration. No em dashes anywhere.
- Conventional commit messages.

## Repo layout

| Path | What |
|---|---|
| `Cargo.toml` | Root workspace, `members = ["crates/*", "apps/aroh/src-tauri"]` |
| `crates/aroh-core` | Row structs (serde), SQLite migrations, HLC, repositories |
| `crates/aroh-<pod>` | One crate per pod that needs Rust logic |
| `apps/aroh` | SvelteKit SPA (adapter-static, Svelte 5 runes, TS) |
| `apps/aroh/src-tauri` | Tauri 2 app crate |
| `apps/aroh/src-tauri/src/commands/<pod>.rs` | One Tauri command module per pod, registered in `lib.rs` |
| `docs/decisions.md` | Settled decisions |
| `Dockerfile.test` | Container that runs the workspace tests |

## Ownership

| Pod | Owns |
|---|---|
| foundation | workspace, `crates/aroh-core`, app shell, `commands/core.rs`, keychain, `docs/decisions.md`, `AGENTS.md`, `Dockerfile.test` |
| sync | `crates/aroh-sync`, `supabase/`, `commands/sync.rs` |
| plugins | `crates/aroh-plugins`, `apps/aroh/src/lib/plugins/`, `commands/plugins.rs` |
| replan | `crates/aroh-replan`, `/replan` route, `commands/replan.rs` |
| goals | `crates/aroh-agents`, `/goals/new`, `/goals/[id]`, `commands/goals.rs` |
| calendar | `/today`, `/calendar` routes, reminders (local notifications), `commands/calendar.rs` |
| trackers | `crates/aroh-intake`, `/log` route, voice input, `commands/intake.rs` |

Change a file another pod owns only through that pod. Shared row structs live in `aroh-core`; ask foundation to change them.

## Adding a pod's Tauri commands

1. Create `apps/aroh/src-tauri/src/commands/<pod>.rs` with `#[tauri::command]` functions, following `commands/core.rs`.
2. Add `pub mod <pod>;` to `apps/aroh/src-tauri/src/commands/mod.rs`.
3. Add the functions to the `tauri::generate_handler![...]` list in `apps/aroh/src-tauri/src/lib.rs`.

## Running tests

Workspace tests, without the Tauri app crate, in a Kiri container. Agent shells may have a sandbox HOME, so plain `kiri` says "Kiri isn't running". Set `REAL_HOME` to your real home directory (for example `/Users/<you>`) and prefix every call with it:

```sh
HOME="$REAL_HOME" kiri build -f Dockerfile.test -t aroh-test .
HOME="$REAL_HOME" kiri run --rm aroh-test
```

Run the app in the iPhone simulator (primary target). Agent shells may have an empty `USER`; set it for xcodegen:

```sh
cd apps/aroh
pnpm install
USER="$(id -un)" pnpm tauri ios dev "iPhone 15 Pro"
```

Run the app on the Mac:

```sh
cd apps/aroh
pnpm install
pnpm tauri dev
```

## Secrets

The Claude API key lives only in the OS keychain (service `aroh`). Never write it to SQLite, logs, sync payloads or the UI.
