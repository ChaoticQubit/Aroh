# Pod 21: Production Deploy, Vercel Hobby Step Mode

**Goal.** Q52 and Q26-27: the server engine runs on Vercel Hobby in step mode against the owner's Supabase project, with an owner runbook (`docs/deploy.md`) and a smoke check the owner can run after the deploy. No CLI tools are installed: the owner does every dashboard step.

**Branch.** `chore/prod-deploy` off master `ef1d40c`.

## Research (2026-11-22, official sources)

| Question | Finding | Source |
|---|---|---|
| Can Vercel run Rust? | Yes. The Rust runtime is in Beta on all plans and runs on Fluid compute. Each handler is a `[[bin]]` whose path is `api/<name>.rs`, built on Vercel from a Git push. | https://vercel.com/docs/functions/runtimes/rust |
| Axum | The official template wraps an Axum `Router` in `tower::ServiceBuilder::new().layer(vercel_runtime::axum::VercelLayer::new())` and passes it to `vercel_runtime::run`. It uses `vercel_runtime = { version = "2.4", features = ["axum"] }`, and `vercel.json` rewrites `/(.*)` to `/api/axum`, so the Router sees the original path. The binary never binds a port: `vercel_runtime` talks to Vercel's function bridge. | https://github.com/vercel/examples/tree/main/rust/axum, https://docs.rs/crate/vercel_runtime/latest |
| Request time on Hobby | 300 s default and maximum with Fluid compute. For Rust, `maxDuration` goes in `vercel.json` under `functions`. | https://vercel.com/docs/functions/configuring-functions/duration, https://vercel.com/docs/functions/limitations |
| Other limits | 2 GB memory and a 250 MB uncompressed bundle on Hobby. One region, `iad1` by default. | https://vercel.com/docs/functions/limitations |
| How the builder finds the bin | `@vercel/rust` runs `cargo locate-project` from the entry file's folder and matches `[[bin]] path` in that manifest. It runs `cargo build --release --bin <name>` in the project Root Directory, with no `-p`. A virtual workspace manifest has no `[[bin]]`, so the entry needs its own package. | https://github.com/vercel/vercel/blob/main/packages/rust/src/index.ts, https://github.com/vercel/vercel/blob/main/packages/rust/src/lib/cargo.ts, https://github.com/vercel/vercel/blob/main/packages/rust/src/lib/compile.ts |
| Monorepo | With a Root Directory set, the build can read files outside it by default ("Include files outside the root directory in the Build Step"). | https://vercel.com/docs/deployments/configure-a-build#root-directory |
| Protection | Standard Protection, the default, protects every URL except production domains. So pg_cron must call the production domain (`<project>.vercel.app`), not a deployment URL. | https://vercel.com/docs/deployment-protection |
| Database from serverless | The direct connection is IPv6 only. The shared pooler in transaction mode (6543) does not support prepared statements, and sqlx prepares every query. The session pooler (5432 on the pooler host) supports them, works over IPv4, and takes no code change. | https://supabase.com/docs/guides/database/connecting-to-postgres |
| JWTs | `sb_secret_...` and `sb_publishable_...` keys are not JWTs. Legacy `anon` and `service_role` are HS256 JWTs signed with the legacy JWT secret. Asymmetric signing keys publish `https://<ref>.supabase.co/auth/v1/.well-known/jwks.json`. | https://supabase.com/docs/guides/api/api-keys, https://supabase.com/docs/guides/auth/signing-keys |
| pg_cron, pg_net, Vault | `net.http_post` is async: requests are sent after the transaction commits. Secrets live in `vault.secrets` and are read from `vault.decrypted_secrets`. | https://supabase.com/docs/guides/database/extensions/pg_net, https://supabase.com/docs/guides/database/vault |

| Hobby allowance | Hobby includes 4 h of Active CPU, 360 GB-hrs of Provisioned Memory and 1 million invocations a month. Two cron calls a minute add about 86,400 invocations a month. | https://vercel.com/docs/functions/usage-and-pricing |

Not evaluated: the community `vercel-rust` runtime, which the official Rust runtime replaces for this use, and a Container runtime. Neither appears in the docs read above.

Verdict: Vercel Hobby can run the Axum engine in step mode through the documented Rust runtime. Alternative not taken: the builder source also has an undocumented "standalone server" mode, for a bin without `vercel_runtime`. It is not in the docs, so it is not used.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Entry crate | `crates/aroh-vercel`, a member through `crates/*`. It holds `api/engine.rs` (`[[bin]] name = "engine"`) and `vercel.json`: a rewrite of `/(.*)` to `/api/engine`, and `maxDuration` 300 for `api/engine.rs`. The Vercel Root Directory is `crates/aroh-vercel`. | The root manifest is a virtual workspace and cannot hold a bin. A crate under `crates/` changes neither the root `Cargo.toml` nor another pod's files. Example profile `lto = "fat"` not copied: build time over a few percent of speed. |
| Shared boot | `main.rs`'s `state` and `transports` move to `aroh_server::boot::app(config)`. The `serve`/`worker` bin and the Vercel bin call it, so both build the same engine. | One constructor. The Vercel entry is 10 lines. |
| Step-mode wake | New migration `20261125000000_step_mode_wake.sql`. (1) The trigger `jobs_wake` on `public.jobs` calls `/v1/hooks/enqueue` through `net.http_post` when a job becomes `queued` from nothing, `done` or `failed`. It skips silently when pg_net or a vault secret is missing, and it never fails the write. (2) When pg_cron, pg_net and Vault exist, it schedules `aroh-push`, every minute, calling `/v1/push/tick`. | Without (1), a new goal or an answer waits up to 60 s for the cron tick. Without (2), push never dispatches in step mode, because `push_loop` runs only in worker mode. Running to queued is the engine's own loop, so it does not wake. |
| DATABASE_URL | The Supabase shared pooler in session mode, port 5432, user `postgres.<ref>`, `?sslmode=require`. | Prepared statements and IPv4, no code change. `require` encrypts without CA checks. `verify-full` needs the Supabase CA bundled (open item). |
| Service JWT | `AROH_JWT_SECRET` is a new random secret that only the engine and the owner hold. `e2e/prod.py mint` signs `{"role":"service_role","iat","exp"}` with it, and the token goes into the Vault secret `aroh_service_jwt`. User tokens: with asymmetric keys, `AROH_JWKS_URL`; with a project still on the legacy secret, `AROH_JWT_SECRET` must be the legacy secret instead, and the same mint step signs with it. | The verifier accepts one HS256 secret. A dedicated secret keeps a real Supabase service-role credential out of Vault and out of the engine. |
| Smoke | `e2e/prod.py smoke` (Python stdlib, env only). Checks: `/healthz` 200; `/v1/keys` 401 without a token; signs in the owner by password, then `/v1/keys` 200 and `/v1/tick` 403 with the user token; with `AROH_SERVICE_JWT` set, `/v1/tick` and `/v1/push/tick` 200. | Same stdlib style as `e2e/smoke.py`; macOS ships `python3`. No secret in the file. |
| Runbook | `docs/deploy.md`, numbered, one action per step: Supabase, Apple, Vercel, Vault, app, smoke, plus proof queries (`cron.job`, `net._http_response`). | Owner request. |

## Tasks

- [x] 1. Failing test `crates/aroh-server/tests/deploy.rs` (ignored, needs `AROH_TEST_PG`). Build `Config::from_vars` over a cloned DB and run `boot::app`. On the served router: `/healthz` 200, `/v1/tick` 401 without a token, 403 with a user token, 200 with a service token, `/v1/push/tick` 200. A job insert and its done-to-queued upsert succeed on plain Postgres with `jobs_wake` installed.
- [x] 2. `aroh_server::boot`, `main.rs` on it, the wake migration. Tests green in Kiri (`sh crates/aroh-server/tests/run.sh`).
- [x] 3. `crates/aroh-vercel` (Cargo.toml, `api/engine.rs`, `vercel.json`), `Cargo.lock`. `cargo build --release --bin engine` passes in Kiri on `rust:1-slim`, run from the workspace root on the Mac's Linux architecture. That is not yet the x86_64 build Vercel runs from `crates/aroh-vercel`.
- [x] 4. `e2e/prod.py` (mint, smoke). Proven against the local e2e stack in Kiri.
- [x] 5. `docs/deploy.md`. Q26-27 row updated in `docs/agent-decisions.md`.
- [x] 6. Boundary check, `git merge master`, DONE.

## Global Constraints

- No comments; docstrings only, 3 to 5 lines, above a declaration. No em dashes. TOML, JSON, YAML, SQL and shell carry no comments.
- No secret values in the repo. Every secret goes in the Vercel env, Supabase Vault or the macOS keychain.
- Kiri with `HOME=/Users/chaoticqubit`, own image names with `p21`, pruned after.
- Conventional commits, commit and never push, no `Co-Authored-By` trailer. Pod 20's calendar and persona app files are not touched.

## Outcome

- `cargo build --release --bin engine` in Kiri `rust:1-slim` finished in 9m 55s with no warnings. `vercel_runtime` 2.4.1 builds with axum 0.8.
- `aroh-server` with Postgres in Kiri: the full run passed 90 tests, and `tests/deploy.rs` then passed 3 on its own (one of them added after the full run). The trigger posts the hook on an insert and on a change from done or failed to queued, never on running to queued. `supabase/tests/run.sh`: rls ok.
- `e2e/prod.py smoke` against the local e2e stack (GoTrue, every migration, the engine through `boot::app`): all 6 checks ok, exit 0. With a bad service token it exits 1, with both ticks at 401. `jobs_wake` is installed on the stack database.
- Not proven here: a real Vercel build and request, Supabase pg_cron and pg_net, and APNs. The first deploy by the owner proves them (runbook sections F, G and I).

### Open items

- `sslmode=require` encrypts but does not check the CA. `verify-full` needs the Supabase CA in the function bundle (`includeFiles`).
- The service token is valid for 365 days and must be renewed by hand (runbook section G).
- The pool is fixed at 10 connections per instance. This is not checked against the session pooler's client limit for the owner's compute size.
- Hobby usage under long agent runs (Provisioned Memory over 250 s ticks) is not measured.
- Vercel build caching for a Root Directory below the workspace target is unverified.
- The Vercel Rust runtime is in Beta.
