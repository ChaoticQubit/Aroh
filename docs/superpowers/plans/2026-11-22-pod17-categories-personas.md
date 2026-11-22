# Pod 17: Goal Categories and the Persona Library

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aroh plans any goal with the right team. The server ships a persona library per category (fitness, learning, music, finance, habits), picks the new-goal team from the goal's category, gives the plugin builder that category's template, and lets the owner copy a built-in persona into their library and edit it there. `aroh-plugins` ships music, learning, finance and habit templates built only from the ComponentSpec contract. Goals get the category filter Calendar already has. "Learn Clair de Lune on piano by March" runs from the new-goal chat to a music dashboard in the simulator.

**Architecture:** The `new-goal` workflow keeps its slot personas `goal-researcher` and `goal-planner`. A new server module `teams.rs` maps a category to a team (a research persona, a plan persona, a template) and rewrites the slots of the parsed `Workflow` before `GraphRun::new`. The run serializes its workflow, so the team survives resume, and `workflows::snapshot` already resolves each name goal folder, then user library, then built-in, so an edited library copy is used by later runs. Category comes from an explicit `category_id` (a seeded id), else a keyword match over title and context, else the general team. A new goal row gets the seeded category id. Persona library endpoints live in a new `personas.rs` over `personas` rows with a null `goal_id`; no new table. Templates are TOML in `crates/aroh-plugins/templates/`, listed in `TEMPLATES`, pinned as tree JSON and resolved by vitest. The app moves Calendar's chip row into one `CategoryFilter.svelte` used by Calendar and Goals.

**Tech Stack:** Rust 2021, toml 1, serde_json, axum 0.8, utoipa 5; Svelte 5, TypeScript, vitest; Kiri; Xcode simulator tools.

## Global Constraints

- Settled decisions: `docs/agent-decisions.md` Q4, Q9, Q11, Q16, Q20, Q33, Q36, Q39, Q41.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell carry no comments. No em dashes.
- `assert!` for internal invariants; input validation returns errors. One copy of every helper.
- Compose only from approved components (Q39). No new component kind, unit, binding source or visual pattern.
- Shared files take additive edits only: `docs/engine-api.yaml` (regenerated from the binary, never by hand), server router, `lib.rs` handler list, RLS table count, `e2e/script.py` rule count.
- `aroh-domain`, `aroh-store`, `supabase/` and `troupekit/` are not edited.
- Tests in Kiri: workspace `HOME=/Users/chaoticqubit kiri build -f Dockerfile.test -t aroh-test-p17 .` then `kiri run --rm aroh-test-p17` (fallback `rust:1-slim` with the source mounted); server `sh crates/aroh-server/tests/run.sh`; frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. One own build at a time (pods 14 and 15 compile too). Remove own images and volumes after each run.
- Simulator device `aroh-p17` (iPhone 15 Pro), dev port 1424, own e2e compose project and ports, fake model transports. `git checkout -- apps/aroh/src-tauri/gen/apple` afterwards.
- Branch `feat/categories-personas`, conventional commits, one commit per task, commit and never push, bring in master with `git merge`, no `Co-Authored-By` trailer.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Team selection | `teams::staff(&mut Workflow, &Team)` rewrites agent nodes whose persona is `goal-researcher` to `team.research` and `goal-planner` to `team.plan` (the `plan` and `revise` nodes) on the parsed workflow before `GraphRun::new`. `intake-interviewer` and `plugin-builder` stay shared. | Persona names must equal their file names and the snapshot is taken before step 0; the run keeps its own workflow copy. No framework change. |
| Teams | general: `goal-researcher`, `goal-planner`, template `habits`. fitness: `goal-researcher`, `goal-planner`, `fitness`. learning: `tutor`, `practice-planner`, `learning`. music: `instrument-teacher`, `practice-coach`, `music`. finance: `budget-coach`, `savings-planner`, `finance`. habits: `goal-researcher`, `habit-coach`, `habits`. | Brief's persona list; the existing two stay the fitness and fallback team so current fakes and tests match. General gets the habits template because fitness trackers (weight, steps) do not fit a career goal. |
| Category source | Explicit `category_id` that equals a seeded id of the user wins (`health-fitness` fitness, `learning` learning, `financial` finance, `habit` habits, `career-projects` general, `hobby-skill` music when the text names music, else general). Otherwise a keyword match over title and context, checked in the order music, finance, fitness, habits, learning; no match is general. | The app's `startGoal` sends no category and the seeds have no Music; seeds are not ours to change. Deterministic and testable. "Learn ... piano" must be music, so music is checked before learning. |
| Rejected: model classifier | An LLM node choosing the team. | The snapshot and persona checks run before the first step, so a classifier would need every team's personas in every run or a graph per team. |
| Goal category | A goal the server creates gets the team's seeded category id (`derived_id` of user and slug, as `seed::categories`); general leaves it null. An existing goal row is not changed. | Without it the filters show the goal as "No category". |
| Slot contract | Every research-slot persona has `goal-researcher`'s `[output]`, tools `web_search`, `web_fetch` and eval `public_sources(sources)`; every plan-slot persona has `goal-planner`'s `[output]` and evals. A test compares the compiled outputs. | The node writes a typed state key; a different output fails the run. |
| New eval | `public_sources(path)`: every `url` under the path is `http` or `https` with a public host (no localhost, no private or link-local address, no credentials). An empty list passes, so a run without search still succeeds. Uses the URL guard troupekit-tools already has when it is public, else a small parser here. | Q33: public URLs only; the research slot had no eval. |
| Safety guardrails | Finance personas carry `Safety: no individual investment, tax or legal advice; refer to a licensed financial professional.` Health personas (`goal-researcher`, `goal-planner`, `habit-coach`) carry `Safety: no medical diagnosis or treatment advice; refer to a doctor for pain, illness or medical conditions.` Every guardrail starting `Safety:` is a safety rule. | Brief; the prefix makes the rule checkable. |
| Library | A library persona is a `personas` row with `goal_id` null, id `derived_id("persona-library:<user>", name, 0)`, so a repeated copy is the same row. | Q9 lookup already reads these rows; no table, RLS count unchanged. |
| Endpoints | `GET /v1/personas`: every built-in Aroh persona and every library persona, one entry per name (library wins), with `name`, `display_name`, `role`, `team` kinds, `source` (`builtin` or `library`), `builtin` (a built-in of that name exists) and `toml`. `POST /v1/personas/{name}/copy`: built-in into the library, 201; 404 unknown built-in, 409 already in the library. `PUT /v1/personas/{name}` `{toml}`: edits a library persona, 200; 404 when not in the library, 422 naming the field when invalid. | Brief: list, copy, edit in the library. Copy first, then edit, so the built-in is never changed. |
| Edit validation | Rejects what would fail at run start or weaken safety: `Persona::parse`, name equals the path, tools in `PLUGIN_TOOLS`, eval names registered, `[output]` equal to the built-in's, every `Safety:` guardrail of the built-in kept. | A bad edit must fail at PUT, not break the owner's next goal. Keeping safety rules is our call, listed as an owner question. |
| Templates | music: countdown, week ring, practice minutes bar chart, tempo line chart, practice streak, pieces log, next sessions. learning: countdown, week ring, session minutes bar chart, modules done stat tile with weekly change, quiz score line chart, study streak, next sessions. finance: savings vs target, savings line chart, spend bar chart, spent today stat tile, next actions. habits: check-in streak, action streak, week ring, today's checks. Tempo, piece progress and quiz score use unit `count` with the real unit in the label. | Only existing sources: no source yields a bool or a module list, and the unit list has no bpm or percent. |
| Template check | Each template validates; its tree is pinned in `apps/aroh/src/lib/plugins/fixtures/<name>.tree.json` (`AROH_PIN=1` rewrites); vitest resolves each pinned tree against a filled snapshot fixture and asserts every item kind comes out and no `bind` is left. | Contract drift on both sides, like pod 12's fixtures. |
| Filters | Calendar's chip row moves into `components/CategoryFilter.svelte` (`categories`, bindable `hidden`); Calendar and Goals use it. `GoalsScreen` takes `categories` and shows only goals of visible categories. `/mockups` passes categories. | Pod 10 built the chips; one copy, no new visual pattern. |
| e2e | Pod 15 put compose project and ports behind `AROH_E2E_PROJECT`, `AROH_E2E_SUPABASE_PORT`, `AROH_E2E_ENGINE_PORT` and the script behind `AROH_E2E_SCRIPT` on its branch. This pod waits for that merge and uses those variables with project `aroh-p17`, its own ports and `e2e/pod17_script.py` (music fake rules). The driver `e2e/pod17.js` signs in, sets the key, starts the music goal in chat, answers, approves, waits for the plugin, logs practice, tempo and piece entries, then captures chat, plan, dashboard, Goals and Calendar filters. | Fresh trackers drop every bound item, so entries come first. Copying pod 15's changes would conflict. |

## File Structure

| Path | Responsibility |
|---|---|
| `crates/aroh-plugins/templates/{music,learning,finance,habits}.toml` | category templates |
| `crates/aroh-plugins/src/lib.rs` | `TEMPLATES` additions |
| `crates/aroh-plugins/tests/plugins.rs` | every template validates, trees pinned |
| `apps/aroh/src/lib/plugins/fixtures/{music,learning,finance,habits}.tree.json` | pinned trees |
| `apps/aroh/src/lib/plugins/resolve.test.ts` | each template resolves with data |
| `crates/aroh-server/personas/{tutor,practice-planner,instrument-teacher,practice-coach,budget-coach,savings-planner,habit-coach}.toml` | category personas |
| `crates/aroh-server/personas/{goal-researcher,goal-planner}.toml` | safety guardrails, `public_sources` eval |
| `crates/aroh-server/src/teams.rs` | `Kind`, `Team`, `TEAMS`, `pick`, `staff`, seeded category ids |
| `crates/aroh-server/src/personas.rs` | library list, copy, edit, edit validation |
| `crates/aroh-server/src/{workflows,evals,engine,api,lib}.rs` | persona list, eval, team in `start_goal`, routes |
| `crates/aroh-server/tests/api.rs` | music flow on Postgres, library endpoints, edited copy used |
| `docs/engine-api.yaml` | regenerated |
| `apps/aroh/src/lib/ui/components/CategoryFilter.svelte` | chip row moved from Calendar |
| `apps/aroh/src/lib/ui/screens/{CalendarScreen,GoalsScreen}.svelte`, `apps/aroh/src/routes/goals/+page.svelte`, `apps/aroh/src/routes/mockups/+page.svelte` | filters |
| `e2e/pod17_script.py`, `e2e/pod17.js`, `e2e/pod17.sh` | simulator flow |
| `docs/screenshots/pod17/*.png` | screenshots |
| `docs/plugins.md` | templates section |

---

### Task 1: Category templates

- [ ] Tests first in `tests/plugins.rs`: `TEMPLATES` holds fitness, music, learning, finance, habits; each validates; each tree equals its pinned JSON. Run, watch it fail.
- [ ] Write the four templates, add them to `TEMPLATES`, pin with `AROH_PIN=1`.
- [ ] vitest: each pinned tree resolves against a snapshot fixture with entries for its trackers; every kind in the tree comes out, no `bind` is left. Watch it fail, then pass.
- [ ] Workspace tests in Kiri, `pnpm check` and vitest in `node:24-slim`. Commit `feat(plugins): music, learning, finance and habit templates`.

### Task 2: Persona library and teams

- [ ] Tests first: every shipped persona parses and loads; research-slot outputs equal `goal-researcher`'s, plan-slot outputs and evals equal `goal-planner`'s; every eval name is registered; finance and health personas carry their `Safety:` text; `public_sources` passes public and empty lists and rejects localhost, private addresses and credentials; `pick` gives music for "Learn Clair de Lune on piano by March", finance for "Save 5000 euro for a trip", fitness for "Run a 5k" and "Six pack by Feb 1", habits for "Meditate every morning", learning for "Learn Spanish to B1", general for "Ship my side project", and an explicit seeded `learning` id beats the text; `staff` rewrites `research`, `plan` and `revise` and nothing else.
- [ ] Implement the seven personas, the guardrail edits, `public_sources`, `teams.rs`, the persona list in `workflows.rs`.
- [ ] Workspace tests in Kiri. Commit `feat(server): category teams and persona library`.

### Task 3: New-goal flow picks the team

- [ ] Tests first in `tests/api.rs`: `POST /v1/goals` with the Clair de Lune title runs with a fake model answering as Instrument teacher and Practice coach; the run row's personas are `goal-planner`-free and hold `instrument-teacher` and `practice-coach`; the plugin builder's request carries the music template; the goal row has the seeded Hobby and Skill id; the run ends with a stored music plugin. The existing "Run a 5k" flow is unchanged.
- [ ] Implement: `Engine::start` takes the parsed workflow; `start_goal` picks the team, staffs the workflow, sets the category on a new goal, puts the team template in `plugin_guide`.
- [ ] `sh crates/aroh-server/tests/run.sh` green. Commit `feat(server): new-goal team by goal category`.

### Task 4: Persona library endpoints

- [ ] Tests first: list shows built-ins with `source = builtin`; copy answers 201, again 409, unknown 404; PUT valid edit answers 200 and the list shows `library`; PUT answers 422 for a changed name, unknown tool, unknown eval, changed output and a dropped `Safety:` guardrail, 404 without a copy; another user sees only built-ins; a new goal run after the edit snapshots the edited text.
- [ ] Implement `personas.rs`, routes, `ApiDoc` entries; regenerate `docs/engine-api.yaml` with the binary.
- [ ] Server tests green. Commit `feat(server): persona library endpoints`.

### Task 5: Category filters

- [ ] vitest first where logic lives: a `visibleGoals(goals, hidden)` style filter if the screen needs one; otherwise the screen filters inline like Calendar.
- [ ] Move the chip row to `CategoryFilter.svelte`, use it in Calendar and Goals, pass `categoryList(s)` on the Goals route and categories in `/mockups`.
- [ ] `pnpm check` and vitest. Commit `feat(app): category filter on goals`.

### Task 6: Simulator acceptance

- [ ] `git merge master`; confirm pod 15's e2e variables are there, else ask `main`.
- [ ] Write `e2e/pod17_script.py` (music fake rules, its own rule count assert), `e2e/pod17.js`, `e2e/pod17.sh`.
- [ ] Create `aroh-p17`, run the dev server on 1424 until it answers 200, build with xcodebuild, install, bring the stack up as `aroh-p17`, run the flow, capture into `docs/screenshots/pod17/`, take the stack down and remove its volumes, `git checkout -- apps/aroh/src-tauri/gen/apple`.
- [ ] Update `docs/plugins.md` templates. Commit `test(e2e): pod 17 music goal from chat to dashboard`.

## Open Items

- Owner: should an edited library copy be allowed to drop a `Safety:` guardrail? This pod says no.
- App screens to browse, copy and edit personas (Settings) are not in this pod; the endpoints are.
- Hobby and Skill goals that are not music get the general team.
