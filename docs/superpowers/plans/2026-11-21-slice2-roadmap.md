# Slice 2 Roadmap

Slice 1 is verified end to end on one fitness goal (commit 443d99a). Slice 2 takes Aroh to any goal: plugin views per goal, real data capture (Apple Health), server push, external calendar triage, the full set of adaptation triggers, more goal categories with a persona library, and the slice 1 leftovers.

## Global constraints

- Decisions in `docs/agent-decisions.md` are settled. Relevant rows: Q10, Q16, Q17, Q19 to Q24, Q32 to Q38, Q41.
- Mockups only for new components or new visual patterns (Q39). Screens built from approved components and the Q41 visual rule need simulator screenshots, not a mockup.
- One pod per feature, own branch and worktree, tests written first, Kiri for tests, iPhone simulator for end to end with a simulator device named after the pod.
- No code comments, only docstrings above declarations, 3 to 5 lines. No em dashes. No `Co-Authored-By` trailer. Commit, never push.
- No provider key in the app, SQLite, logs, sync payloads or UI.
- `troupekit/` stays free of the host app name; framework changes go through the boundary check.
- The Mac disk and the Kiri VM are shared: remove your own images and volumes after each run.

## Pods

| Pod | Feature | Owns | Depends on |
|---|---|---|---|
| 12 | Plugins: TOML bundles of trackers plus a view tree of our components, validated, versioned, enabled per goal; a plugin-builder persona generates one per goal; the app renders the goal dashboard from it | `crates/aroh-plugins`, `apps/aroh/src/lib/plugins/`, `commands/plugins.rs`, server plugin module and endpoints | slice 1 |
| 13 | Apple Health import into trackers and server push via APNs with the local 24 h fallback | health and push modules in app and server, `commands/health.rs` | slice 1 |
| 14 | External calendar triage: EventKit read, triage persona groups events, owner keeps or ignores per group, rules, busy blocks, new events matched on device | calendar plugin and triage modules, `commands/calendar_import.rs`, server triage workflow | slice 1 |
| 15 | Adaptation triggers and learning: tracker rules, scheduled reviews on the server tick, action-event triggers, learning from repeated edits, Inbox insights | server triggers and learning modules, insights endpoint, Inbox insights view | slice 1 |
| 16 | Slice 1 leftovers: verified prices, Realtime instead of polling, goal chat screen, 06:00 hour label, release iOS build path | as listed, app and engine edges | 12 to 15 merged |
| 17 | Other goal categories and the persona library: learning, finance, music and habit templates, category filters | personas, workflows and plugin templates per category | 12 merged |

Pods 12 to 15 run in parallel. Pods 16 and 17 start after their dependencies merge. Shared files (root `Cargo.toml`, `lib.rs` handler list, `commands/mod.rs`, `docs/engine-api.yaml`) take additive edits only; the leader resolves conflicts at merge.

## Acceptance (slice 2)

- A non-fitness goal (for example "learn Clair de Lune on piano by March") runs from new-goal chat to a plugin dashboard with its own trackers and views.
- Apple Health weight and workouts land as tracker entries; a push notification reaches the simulator from the server.
- Device calendar events are triaged into groups, kept groups block planning, and the plan avoids them.
- A missed action or a tracker rule triggers a replan proposal without the owner asking; a scheduled review runs with the app closed.
- Owner edits repeated three times produce a preference proposal.
