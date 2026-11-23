# Open-source release: audit and plan (Q73, phase A)

Pod 39, 2026-11-23. Goal: publish Aroh under Apache-2.0, production grade (Q73). Phase A is this audit and plan; no product code changed. The security review (security-audit skill) is a later phase.

This file holds no secret values, identifiers or personal data, only `file:line` and the kind of value. The scan in section 1 is a regex pre-screen over text files. A real secret scanner run in CI (section 3) is the proof.

## Summary: top blockers

| # | Blocker | Why it blocks | Section |
|---|---|---|---|
| 1 | Publishing route for the history | The private GitHub repo holds 181 commits on its 9 remote refs: 174 with a personal author address, 6 with a real name, 7 pull requests with review threads, and the `prod` branch wired to Vercel. The tree carries 163 MB of screenshots and internal agent docs. Making that repo public exposes all of it | 1.4 |
| 2 | No in-app account deletion | The app offers "Create account" (`apps/aroh/src/lib/ui/screens/SettingsScreen.svelte:137`) and a grep of the app, Tauri commands, server and migrations finds no account deletion path. App Review Guideline 5.1.1(v) requires it | 5 |
| 3 | Strangers on the hosted server | Sign-ups are off on the hosted project (`docs/deploy.md` step 17), Vercel Hobby is for non-commercial use, and multi-user hosting with abuse limits is out of scope in Q73. A public App Store build that defaults to the hosted server needs an owner decision | 2.4, 5 |
| 4 | No one-command self-host | Q73 lists it first. No production engine image, no storage service in the compose stack, no Supabase CLI config, `docs/deploy.md` lists 7 of 12 migrations, and the hosted project's URL and key are compiled in | 2 |
| 5 | No CI at all | No `.github/` directory exists on any ref. Q73 requires the full suite on every PR plus dependency and secret scanning | 3 |

Also required, small: the licence is not declared. Root `Cargo.toml:9` says `license = "UNLICENSED"`; no `LICENSE` or `NOTICE` at the root; 7 crates have no `license` field; `apps/aroh/package.json` has no licence; MPL-2.0 crates ship inside the app binary and need third-party notices (section 4).

## 1. Secrets and identity sweep

Method: a regex pre-screen over every tracked text file at the branch tip and over every added line in `git log --all -p` (595 commits: every local branch plus the 9 remote refs, which hold 181 of them), for private key blocks, Supabase secret and publishable keys, JWTs, Anthropic, OpenAI, GitHub, AWS, Google and Slack token shapes, Supabase project URLs and the bare project ref, pooler hosts, Postgres URLs with a password, Vercel domains, email addresses, the owner's home path and username, the Apple team ID, 10-character key IDs, `AuthKey_` file names, password assignments, the bundle id and the App Store name. Also: file names ever added (`.p8`, `.p12`, `.pem`, `.key`, `.env`, `.mobileprovision`) and commit metadata.

Result: no live secret (service key, secret key, provider key, `.p8`, database password, JWT signing secret) was found in the tree or in history. Every value found in history is also present at the tip, except two false positives (a keychain lookup in the deleted `crates/aroh-sync/live.sh` and a fake URL in an old `crates/aroh-sync/src/config.rs` test). No secret-named file was ever committed.

Not scanned: 728 tracked images under `docs/screenshots` (163 MB) and `docs/demo/aroh-mvp-demo.mp4` (8 MB). They are unverified; the owner must check them, or they stay out of the public tree (1.4).

### 1.1 Findings at the tip

| Kind | Where | Class | Action |
|---|---|---|---|
| Supabase project URL (hosted project ref) | `crates/aroh-app/src/local.rs:37`, `crates/aroh-app/tests/app.rs:359`, `docs/superpowers/plans/2026-11-22-pod25-build1-quick-fixes.md:24` | move to config | Build-time default (2.1); test uses a fixture value |
| Supabase publishable key (hosted) | `crates/aroh-app/src/local.rs:38`, `crates/aroh-app/tests/app.rs:360`, `crates/aroh-sync/src/auth.rs:207` | fine public by design, move to config | Publishable keys are meant to ship in clients. Q73 asks for no hard-coded project keys in the build, so it moves to the build-time default; tests use a fixture |
| Supabase publishable key (fixture) | `crates/aroh-sync/src/storage.rs:121` and 4 more lines | fine public | Test value |
| Hosted engine domain | `crates/aroh-app/src/local.rs:39`, `crates/aroh-app/tests/app.rs:44,361,371`, `docs/agent-decisions.md:79`, the pod 25 plan | move to config | Build-time default (2.1) |
| Example engine domain | `docs/deploy.md:10` | fine public | Example only; check it is not a live project before publishing |
| EC P-256 private key block | `crates/aroh-server/src/auth.rs:221` | fine public, replace | Test fixture: inside `#[cfg(test)]` (line 181), paired with its public JWK in the same test (line 234), added in 4cc4159. Secret scanners will flag it; generate it at test time as `crates/aroh-server/src/push.rs:484` already does |
| Private key header in format strings | `crates/aroh-server/src/push.rs:490,525`, `crates/aroh-server/tests/push.rs:21`, `docs/deploy.md:103` | fine public | Keys generated at runtime, or prose |
| `sk-ant-` shaped strings | `crates/aroh-server/src/vault.rs:304`, `crates/aroh-server/tests/api.rs:21`, `crates/aroh-server/tests/store.rs:75-141`, `crates/aroh-server/tests/triggers.rs:59`, `e2e/gate4.js:266`, `e2e/sim.sh:34` | fine public | Fake fixtures (test, alpha, bravo, e2e). Add them to the secret scanner allowlist |
| Postgres URLs with a password | `crates/aroh-server/tests/compose.yaml:20`, `e2e/compose.yaml:30,60,110` | fine public | Local test containers only |
| Password literals | `crates/aroh-sync/src/auth.rs:163-164`, `e2e/down.sh:10` | fine public | Test placeholders |
| Apple team ID | `apps/aroh/ios-release.sh:14` (default of `AROH_TEAM_ID`), `README.md:67`, `docs/deploy.md:191,200`, `docs/agent-decisions.md:77`, `docs/superpowers/plans/2026-11-22-pod24-testflight.md:3,12` | move to config | Not a secret (it is in every signed binary), but it ties builds to the owner. `AROH_TEAM_ID` becomes required for `testflight` mode; docs use a placeholder |
| Owner home path | 34 lines in 19 files: `AGENTS.md:53-54`, `CLAUDE.md:53-54`, `README.md:18-19`, `docs/deploy.md:197,215`, 15 files under `docs/superpowers/plans/` | must remove | Replace with `$HOME` or drop; plans leave the public tree (1.4) |
| Owner GitHub username | the above plus `docs/deploy.md:89`, `troupekit/Cargo.toml:9`, two plan files | must remove or keep deliberately | The repo URL is public anyway once published; decide which org or user hosts it |
| Bundle id | 42 lines in 25 files (app config, Xcode project, server tests, e2e scripts, docs) | fine public | Belongs to the owner's App Store app; self-hosters override it (2.1) |
| App Store name | `docs/deploy.md:192` | fine public | |
| Email addresses | `e2e/gate4.js`, `docs/gate/*.txt`, tests, mock data, `supabase/tests/rls_rebuild.sql` | fine public | All `example.com`, `example.org`, `aroh.test` or fake; the rest are `@2x.png` asset names |

### 1.2 Findings in history only

| Kind | Where | Class |
|---|---|---|
| Commit author email (personal address) | 174 commits on the remote refs (more on local-only branches) | must remove before publishing (identity) |
| Real name in author field | 6 commits (GitHub web merges) | must remove before publishing (identity) |
| Bot author on a `.invalid` domain | 1 commit | fine |

No secret value exists only in history, so no key needs rotating because of the repo. Rotation is still prudent for anything that ever passed through agent transcripts or chats, which this sweep cannot see.

### 1.3 Internal material that should not ship

| Path | Size or count | Why |
|---|---|---|
| `docs/screenshots/` | 728 files, 163 MB | Unverified for personal data; bloats every clone |
| `docs/demo/aroh-mvp-demo.mp4` | 8 MB | Same |
| `docs/superpowers/plans/` | 716 KB | Agent plans with the owner's paths and internal process |
| `docs/gate/`, `docs/pods/`, `docs/agent-decisions.md` | small | Internal process records; keep in the private repo or rewrite as public docs |
| `AGENTS.md`, `CLAUDE.md` | 2 files | Agent workflow with the owner's home path; replace with a public `CONTRIBUTING.md` and a short agent file without local paths |
| `docs/open-source.md` (this file) | 1 file | Maps where every identity value sits; remove or rewrite once phase B fixes them |
| `e2e/pod*.js`, `e2e/pod*.sh` | per-pod scripts | Keep only the stack, smoke and drive scripts a contributor runs |

### 1.4 History: rewrite or fresh root

Recommendation: **publish from a fresh squashed root in a new public repository**, and keep the current private repo as the archive.

| Option | For | Against |
|---|---|---|
| Fresh squashed root, new public repo (recommended) | No author emails, no real name, no 212 MiB pack, no internal docs; one signed commit satisfies the Q55 ruleset; PRs and `prod` stay private | Loses public blame history; Vercel and the rulesets must be set up on the new repo |
| `git filter-repo` rewrite of the existing repo, then make it public | Keeps history | Rewrites every commit anyway (author emails); GitHub keeps PR refs (`refs/pull/*`) and review threads of the old commits, which a force-push does not remove; the 9 remote refs and their PRs all need handling |
| Make the existing repo public as is | Nothing to do | Exposes identity, internal docs and all PR history |

Steps for the recommended route (phase B, owner runs the GitHub parts):

1. On a release branch, remove the material in 1.3 and fix the findings marked "must remove" and "move to config".
2. Create the new public repository (owner: name and owner account or org).
3. `git checkout --orphan public && git commit -S` with a public author identity (GitHub noreply address); push only that branch as `master`.
4. Re-create the rulesets (pull request, signed commits) and the Vercel link on the new repo.
5. Keep developing in the public repo from then on; the private repo becomes read-only.

## 2. Self-host

### 2.1 What ties a build to the owner's hosted project

| Value | Where | Today | Proposed override |
|---|---|---|---|
| Supabase URL, publishable key, engine URL | `crates/aroh-app/src/local.rs:36-40` (`DEFAULTS`, Q59) | compiled in; editable in Settings > Account and Server and budget | Read at build time with `option_env!("AROH_DEFAULT_SUPABASE_URL")`, `AROH_DEFAULT_SUPABASE_ANON_KEY`, `AROH_DEFAULT_SERVER_URL`. The official release build sets them to the hosted project; a source build without them starts with empty settings and the first-run flow asks for a server. Q73 keeps the hosted server as the default of the official build |
| Bundle id `io.aroh.app` | `apps/aroh/src-tauri/tauri.conf.json:5`, `apps/aroh/src-tauri/gen/apple/project.yml:3,14`, `apps/aroh/src-tauri/gen/apple/aroh.xcodeproj/project.pbxproj:290,440`, `apps/aroh/ios-release.sh:72,79` | fixed | A self-hoster who ships to their own phone needs their own bundle id: a `tauri.conf` override file (`tauri build --config`) plus regenerating the Xcode project. Document it; keep `io.aroh.app` for the official build |
| APNs topic | engine env `AROH_APNS_TOPIC` | already config | Matches the self-hoster's bundle id; push is off when all `AROH_APNS_*` are unset (`crates/aroh-server/src/push.rs:85-100`) |
| Apple team | `apps/aroh/ios-release.sh:14` | default to the owner's team | Make `AROH_TEAM_ID` required in `testflight` mode |
| App Store name, SKU | `docs/deploy.md:192` | doc only | Placeholder in public docs |
| Engine secrets | `DATABASE_URL`, `AROH_MASTER_KEY`, `AROH_JWT_SECRET`, `AROH_JWKS_URL`, `AROH_APNS_*` | already env | Ship `.env.example` with names and how to generate each |
| Engine options | `AROH_BIND`, `PORT`, `AROH_JWT_AUDIENCE`, `AROH_SEARCH_TOML`, `AROH_ALLOW_CUSTOM_BASE_URL` | env | Document; `AROH_ALLOW_CUSTOM_BASE_URL` stays off by default |
| Test and dev only | `AROH_FAKE_SCRIPT`, `AROH_DEV_SCRIPT` | behind the `fake-transports` feature (`crates/aroh-server/src/boot.rs:62-66`) | Never set in a release build; CI checks the release binary builds without the feature |
| Repository URL of troupekit | `troupekit/Cargo.toml:9` | points to a repo that does not exist | Point to the public repo path or remove |

The app side already accepts any server: Supabase and engine URLs must be https, or http on localhost (`crates/aroh-sync/src/config.rs:6,26`), and all network calls run in Rust, so the webview CSP (`apps/aroh/src-tauri/tauri.conf.json:24`) does not limit hosts.

### 2.2 One-command setup paths

**Path A: Docker compose (self-contained).** Most of it already exists in `e2e/compose.yaml`: Postgres 17 with pgvector and wal2json, GoTrue, PostgREST, Realtime, an nginx gateway, a migration job and the engine. Missing today:

| Gap | Detail |
|---|---|
| Production engine image | The e2e engine runs `cargo run --features fake-transports -- worker` in `rust:1-slim`. Needed: a multi-stage `Dockerfile` that builds `aroh-server` in release without `fake-transports` into a slim runtime image |
| Worker mode instead of cron | pg_cron, pg_net and Vault are not in the image; the migrations skip the cron and wake blocks when they are missing (`supabase/migrations/20261123150000_triggers_learning.sql:97-99`, `20261125000000_step_mode_wake.sql:14,37-39`). In `worker` mode the engine runs its job loop and its push loop in process (`crates/aroh-server/src/main.rs:64`), so the self-host compose runs `worker` and needs no cron |
| Storage | No storage-api service in the compose stack; `20261127020000_goal_files_bucket.sql` targets `storage.objects`. Attachments do not work until a `supabase/storage-api` service and gateway route are added |
| Secrets and keys | `e2e/keys.py` already generates the JWT secret, master key and anon key; reuse it in a `setup.sh` that writes `.env` once |
| TLS | The app needs https for a non-localhost server. Add a Caddy service with automatic certificates, or document a reverse proxy |
| Sign-up policy | GoTrue sign-ups on or off by env (`GOTRUE_DISABLE_SIGNUP`) |
| Kiri vs Docker | Scripts call `kiri`; public scripts use `docker compose` and accept `DOCKER=kiri` |

Target command: `./selfhost/setup.sh` (writes `.env`) then `docker compose -f selfhost/compose.yaml up -d`; the app points at `https://<host>` and `https://<host>/engine`.

**Path B: Supabase cloud plus Vercel.** `docs/deploy.md` is the manual version. Missing today:

| Gap | Detail |
|---|---|
| Supabase CLI project | No `supabase/config.toml`, so `supabase link` and `supabase db push` do not work; migrations are pasted by hand |
| Stale migration list | `docs/deploy.md` section C lists 7 of the 12 files in `supabase/migrations/` (missing `20261126000000_run_limits` to `20261127020000_goal_files_bucket`) |
| Vault and token step | `e2e/prod.py mint` plus SQL by hand; fold into one script that takes the engine URL and secret |
| Vercel deploy | Root dir `crates/aroh-vercel` and env set by hand; add a Deploy button or `vercel` CLI script and document the 10-minute first build |
| Owner-specific steps | Steps 27-28 (push to the owner's `prod` branch, import the owner's repo) and section J (owner team) become generic |

### 2.3 Docs to write

`docs/self-host.md` (both paths), `.env.example`, a refreshed `README.md` (it still describes `rig-core`, a keychain-held Claude key and no self-run server), and a refreshed `docs/decisions.md` stack table (it still says "No self-run server" and "Claude API key in the OS keychain", while the engine now stores provider keys encrypted under `AROH_MASTER_KEY`).

### 2.4 The hosted server for strangers

The hosted project runs on Vercel Hobby and Supabase with sign-ups disabled (`docs/deploy.md` step 17). The engine holds every account's provider key. Q73 excludes a multi-user hosted service with rate limits, abuse protection and per-user cost caps. A public App Store build that defaults to the hosted server therefore needs one of: sign-ups on with a paid plan and a per-account limit, an invite-only hosted server, or no hosted default in the public build (own-server only). Owner decision D3.

## 3. CI

Today: no `.github/` directory on any local or remote ref. Tests run locally in Kiri containers (`Dockerfile.test`, `crates/aroh-server/Dockerfile.test`, `crates/aroh-agents/Dockerfile.test`, `crates/aroh-reminders/Dockerfile.test`, `troupekit/Dockerfile.test`, `supabase/tests/run.sh`).

Proposed workflows, all on GitHub-hosted runners (owner rule: full suite on the release PR; Q73: every PR on the public repo):

| Workflow | Runner | Content |
|---|---|---|
| `rust.yml` | ubuntu-latest | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` excluding `aroh` and the three `tauri-plugin-aroh-*` crates (as `Dockerfile.test`); troupekit separately with `scripts/check-boundary.sh`; `cargo build -p aroh-server --release` without `fake-transports` |
| `server-pg.yml` | ubuntu-latest | Postgres service container (pgvector pg17), `cargo test -p aroh-server -- --include-ignored` with `AROH_TEST_PG` (as `crates/aroh-server/Dockerfile.test`) |
| `rls.yml` | ubuntu-latest | `supabase/tests` RLS suite against a pgvector service container; `run.sh` needs a `docker` path instead of `kiri` |
| `web.yml` | ubuntu-latest | pnpm install with frozen lockfile, `pnpm check` (svelte-check), `vitest run` (27 test files under `apps/aroh/src`; `package.json` has no `test` script yet), `pnpm build` |
| `app-build.yml` | macos-latest | `cargo check -p aroh` and an unsigned iOS simulator build; slow, so on the release PR or nightly only |
| `e2e-smoke.yml` | ubuntu-latest | Bring up the compose stack, run `e2e/smoke.py` (sign-up, RLS isolation, engine health and auth), then `nuclei` against the gateway and engine, severity `medium,high,critical` (owner rule). The simulator flows (`e2e/sim.sh`) need macOS and stay out |
| `deps.yml` | ubuntu-latest | `cargo deny check licenses advisories bans sources` (Rust workspace and troupekit), `pnpm audit --prod`; weekly schedule plus every PR touching lockfiles; Dependabot for cargo, npm and GitHub Actions |
| `secrets.yml` | ubuntu-latest | gitleaks on the PR diff and a full-history scan on the release PR, with an allowlist for the fixtures in 1.1 |

Tools are pulled inside the workflows by pinned actions; nothing is installed on the owner's Mac.

Owner actions: create the public repo; enable GitHub secret scanning, push protection, Dependabot alerts and private vulnerability reporting; set branch rulesets with the required checks above; decide whether CI holds any secret (none needed for the suites above; nuclei runs against the local stack).

## 4. Licensing

### 4.1 Dependencies

Rust, root workspace (`cargo metadata --locked`, 685 third-party packages): all are permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, Unicode-3.0, CC0, BSL-1.0, MIT-0) or dual-licensed with a permissive option, except:

| Crate | Licence | Pulled by | Compatible with Apache-2.0 |
|---|---|---|---|
| `cssparser` 0.37 and 0.38, `cssparser-macros`, `selectors` 0.38 and 0.41, `dtoa-short` | MPL-2.0 | `tauri` (shipped app) | Yes when used unmodified; file-level copyleft. The app must ship their notices and point to their source |
| `option-ext` | MPL-2.0 | `tauri`, `tauri-plugin-aroh-calendar` | Same |
| `webpki-roots`, `webpki-root-certs` | CDLA-Permissive-2.0 | `aroh-agents`, `aroh-replan`, the app | Yes (permissive data licence); allow in cargo-deny |
| `r-efi` | MIT OR Apache-2.0 OR LGPL-2.1+ | transitive | Yes (choose MIT or Apache-2.0) |

troupekit workspace (227 packages): same picture, no MPL.

JavaScript (`apps/aroh`, read from the installed `node_modules` of the main checkout, 106 packages; `pnpm licenses` was not run because the worktree has no install): MIT, Apache-2.0, ISC, BSD-3-Clause, BlueOak-1.0.0, plus `lightningcss` (MPL-2.0, build tool, not shipped) and `svelte-toolbelt` (no `license` field; its LICENSE file is MIT). No GPL in either ecosystem. CI `cargo deny` and a pnpm licence check make this binding.

### 4.2 Files to add

| File | Content |
|---|---|
| `LICENSE` | Apache-2.0 full text |
| `NOTICE` | Copyright line, plus attribution for bundled third-party code |
| `THIRD_PARTY_NOTICES` (generated, shipped in the app's About screen) | Generated with `cargo about` or equivalent in CI; covers the MPL-2.0 crates |
| `CONTRIBUTING.md` | Setup, tests, commit style, DCO or CLA choice (owner decision D6), no-comment and no-em-dash style rules |
| `SECURITY.md` | Private reporting through GitHub advisories, supported versions, scope (server, sync, key storage) |
| `CODE_OF_CONDUCT.md` | Contributor Covenant 2.1 |
| `.github/ISSUE_TEMPLATE`, `pull_request_template.md` | Optional |

Manifest changes: root `Cargo.toml:9` from `UNLICENSED` to `Apache-2.0`; add `license.workspace = true` to the 7 crates without a licence field (`aroh-agents`, `aroh-intake`, `aroh-reminders`, `aroh-replan`, the three `tauri-plugin-aroh-*`); `"license": "Apache-2.0"` in `apps/aroh/package.json`.

### 4.3 troupekit

`troupekit/` is a plain directory in this repo, not a git subtree (no `git-subtree-dir` trailer in any commit) and not a submodule. It is excluded from the root workspace (`Cargo.toml:4`), has its own `Cargo.lock` and test container, is licensed MIT OR Apache-2.0 with both licence files, and `aroh-server` uses it by path (`crates/aroh-server/Cargo.toml:18-23`). Its `repository` field points to a GitHub repo that does not exist. Options: publish it inside the Aroh repo as is (simplest; its dual licence is compatible), or split it into its own repo and crates.io release later. Owner decision D5.

## 5. App Store public release checklist

Guideline numbers are from the App Review Guidelines, last updated 2026-10-16, read on 2026-11-23.

| Item | Guideline | State | Needed |
|---|---|---|---|
| Account deletion in the app | 5.1.1(v) "If your app supports account creation, you must also offer account deletion within the app" | missing | Settings > Account > Delete account: delete the auth user (server endpoint with the service role), rows in every table, stored provider keys, devices, storage objects, then the local SQLite and keychain entries. Blocker |
| Privacy policy | 5.1.1(i) link in App Store Connect and in the app; data collected, third parties, retention and deletion | missing | A hosted policy page and a link in Settings |
| Sharing with third-party AI | 5.1.2(i) disclose and get explicit permission before sharing personal data with third parties, including third-party AI | missing | A consent screen before the first agent run naming the model provider and what is sent (goals, answers, calendar context, tracker entries) |
| Health data | 5.1.3(i) no use or disclosure for advertising or data mining; disclose the specific health data; 5.1.3(ii) no personal health information in iCloud | to verify | A HealthKit import writes local rows and then triggers a sync (`apps/aroh/src-tauri/src/commands/health.rs:44-46`); which health rows reach Supabase is to confirm and server tools reference health (`crates/aroh-server/src/tools.rs`). The security phase confirms whether health values reach the model provider; the policy and consent screen must say so |
| No forced permissions | 5.1.2(i) "may not require users to enable system functionalities (such as push notifications, location services ...)" | to verify | The first-launch setup flow (Q59) must let each permission be skipped; the app must work without them |
| Background location | 2.5.4 background services only for their intended purposes | to verify | `NSLocationAlwaysAndWhenInUseUsageDescription` is set; review notes must explain the place-based reminders that need it |
| Demo account and live backend | 2.1(a) demo account info and back end turned on; or a demo mode with Apple's approval | missing | A review account on the hosted server with a provider key the engine holds, plus review notes; or a demo mode that runs with scripted models |
| Sign in with Apple | 4.8 applies only to third-party or social login | not required | Email and password only today |
| App privacy labels (App Store Connect) | App Store Connect questionnaire | missing | Contact info (email), health and fitness, location (if sent to the server), user content (goals, notes, files), identifiers (device push token); linked to the user; not used for tracking |
| Privacy manifest | Apple privacy manifest files (`PrivacyInfo.xcprivacy`) | missing, requirement unverified in this pass | No `PrivacyInfo.xcprivacy` in the Xcode project; confirm against Apple's privacy manifest documentation, then declare required-reason API use (for example file timestamps, UserDefaults) by the app and its native plugins |
| Export compliance | App Store Connect | done | `ITSAppUsesNonExemptEncryption` false in both `Info.plist` files |
| Push entitlement | | done for release | `aps-environment` is `development` in `aroh_iOS.entitlements`; the archive export sets production (TestFlight works today) |
| Usage strings | Info.plist purpose strings (not a numbered guideline) | present | Calendars, camera, health share and update, location, microphone, photos, speech recognition; review them for clear purpose wording |
| Onboarding for strangers | Q73 | missing | First run choose hosted server or own server, create account, enter provider key, consent screen |
| Age rating, support URL, marketing URL, screenshots with fictional data (2.3.9) | 2.3 | missing | Owner, in App Store Connect |

## 6. Owner decisions needed

| # | Decision | Recommendation |
|---|---|---|
| D1 | Publish from a fresh squashed root in a new public repo, keeping the private repo as archive | Yes (1.4) |
| D2 | Which GitHub user or org owns the public repo, and the public author identity for commits | Owner's choice; use a GitHub noreply address |
| D3 | Hosted server for strangers: open sign-ups on a paid plan with limits, invite-only, or no hosted default in the public build | Invite-only or own-server until limits exist (Q73 excludes multi-user hosting) |
| D4 | Hosted defaults moved to build-time env, set only for the official release build | Yes (2.1) |
| D5 | troupekit stays in the Aroh repo or becomes its own repo | Stays in the repo for the first release; fix its `repository` field |
| D6 | DCO sign-off or a CLA for contributions | DCO (lighter, fits Apache-2.0) |
| D7 | Screenshots, demo video, agent plans, gate and pod records stay private | Yes (1.3) |
| D8 | Demo account with an owner-paid provider key or a scripted demo mode for App Review | Demo account on the hosted server with a capped key |

## 7. Phase B plan (after the owner's decisions)

1. Licence files and manifest fields (4.2).
2. Hosted defaults to build-time env, test fixtures instead of the real values, `EC_KEY` generated at test time, `AROH_TEAM_ID` required (2.1, 1.1).
3. Public docs: `README.md`, `docs/self-host.md`, `.env.example`, refreshed `docs/decisions.md` and `docs/deploy.md` (all 12 migrations), `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`; remove owner paths.
4. Self-host compose with a release engine image, storage-api, TLS proxy and `setup.sh`; Supabase CLI `config.toml` (2.2).
5. CI workflows and scanners (3).
6. Security review of server, sync and key storage with the security-audit skill (Q73).
7. App Store items: account deletion, privacy policy, AI consent screen, privacy manifest, onboarding, review notes (5).
8. Fresh root and publish (1.4).
