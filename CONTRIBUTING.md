# Contributing to Aroh

Thanks for helping. This guide covers setup, tests, style and how a change gets in. By taking part you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Report security problems privately, as [SECURITY.md](SECURITY.md) describes, never in an issue.

## Setup

- Rust (stable, through rustup), Node.js and pnpm.
- For the iPhone app: macOS with Xcode. For the Mac app: macOS.
- Docker (or a compatible CLI) for the container test suites and the self-host stack.

```sh
cd apps/aroh
pnpm install
pnpm tauri dev
```

A build from source has no server set. Run your own with [docs/self-host.md](docs/self-host.md) and enter it in Settings, or follow the first-launch **Use my own server** step.

The layout and the settled decisions are in `README.md`, `AGENTS.md` and `docs/decisions.md`. Read `docs/decisions.md` before changing behaviour.

## Tests

Write the test first, watch it fail, then make it pass. Run the tests for the code you changed before you open a pull request:

```sh
cargo test --workspace --exclude aroh --exclude tauri-plugin-aroh-speech --exclude tauri-plugin-aroh-ios --exclude tauri-plugin-aroh-calendar
cd apps/aroh && pnpm check && pnpm test
```

The same workspace run in a container: `docker build -f Dockerfile.test -t aroh-test . && docker run --rm aroh-test`. The engine's Postgres tests run with `AROH_TEST_PG` pointing at a Postgres 17 with pgvector (see `crates/aroh-server/tests/compose.yaml`). troupekit has its own workspace and container under `troupekit/`.

GitHub Actions runs the full suite (`.github/workflows/ci.yml`) on every pull request into `master` and `prod` and every night: Rust workspace and troupekit tests, the engine and RLS suites on Postgres, svelte-check, vitest and the web build, cargo-deny, cargo-audit and pnpm audit, a check that `THIRD_PARTY_NOTICES.md` is current, the DCO check, and the self-host stack with a smoke test and a nuclei scan.

When you change Rust dependencies, regenerate the notices: `python3 scripts/third_party_notices.py > THIRD_PARTY_NOTICES.md`.

## Style

- No code comments. The one allowed form is a docstring directly above a declaration (`///` in Rust, `/** */` in TypeScript and Svelte, `"""` in Python), 3 to 5 lines: what it does, inputs, outputs. Files with no docstring form (YAML, shell, SQL, Markdown, TOML) carry none; explain them in `docs/` or the pull request.
- No em dashes anywhere; use a hyphen or rephrase.
- Assertions for internal invariants (`assert!` in Rust, `assert` in Python); input validation at trust boundaries is separate and stays.
- Never log, store or sync a provider key or token. Secrets live only in the engine's encrypted store, the OS keychain or the server environment.

## Commits and pull requests

- [Conventional commits](https://www.conventionalcommits.org/): `feat(scope): ...`, `fix(scope): ...`, `chore(scope): ...`, `docs(scope): ...`.
- Keep a pull request small and about one thing. Describe what changed, why, and how you tested it.
- Every commit carries a Developer Certificate of Origin sign-off (below). CI rejects a pull request with an unsigned commit.

## Developer Certificate of Origin

By signing off a commit you certify the [Developer Certificate of Origin 1.1](https://developercertificate.org/): that you wrote the change or have the right to submit it under the project's licence. Sign off with `-s`:

```sh
git commit -s -m "fix(sync): retry a dropped Realtime socket"
```

This adds `Signed-off-by: Your Name <you@example.com>` with your git name and email, which must match the commit author. To sign off commits you already made: `git rebase --signoff master`, then force-push your branch.

## Licence

Aroh is licensed under the [Apache License 2.0](LICENSE). By contributing you agree that your contributions are licensed under it. troupekit (`troupekit/`) is licensed under MIT OR Apache-2.0.
