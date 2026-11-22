# Aroh plugins

A plugin is one TOML bundle per goal: tracker schemas, a view layout built only from Aroh's component kinds and bound to those trackers, and optional personas and tool names. No plugin code runs. `crates/aroh-plugins` parses and validates bundles; the app resolves bindings and renders the view with the approved components.

## Lifecycle

1. The new-goal workflow writes the approved plan, then the `plugin-builder` persona writes a bundle. The `valid_plugin` eval checks it; a failing bundle gets one repair round with the validator's errors. `store_plugin` validates again and stores it as version 1, enabled.
2. Owners and agents store new versions with `PUT /v1/goals/{goal_id}/plugins` or, on device, `plugins_save`. A version is never edited in place.
3. One `plugins` row is one version. A goal's dashboard is its enabled row with the highest version. With none enabled, or when the active bundle fails validation on device, the goal shows the derived dashboard.
4. `POST /v1/plugins/{plugin_id}/enable` or the Dashboard picker on the goal screen switches versions. Rows sync like every other table.
5. Storing a bundle creates the trackers it declares that the goal lacks, matched by name. The server also stores the bundle's personas as goal personas; the device leaves them in the bundle.

## A first bundle

```toml
aroh_plugin = 1
name = "water"
title = "Water"

[[trackers]]
name = "Water"

[[trackers.fields]]
key = "ml"
label = "Water"
kind = "number"
unit = "ml"
target = 2000

[[sections]]
title = "Today"

[[sections.items]]
kind = "counter"
label = "Water"
unit = "ml"
value = { bind = "today_total", tracker = "Water", field = "ml" }
target = { bind = "target", tracker = "Water", field = "ml" }
```

## File shape

Every table is closed: an unknown key anywhere is an error.

| Key | Required | Rule |
|---|---|---|
| `aroh_plugin` | yes | Format version. Only `1` exists. |
| `name` | yes | Slug: a-z first, then a-z, 0-9, `-`, `_`; 2 to 40 characters. |
| `title` | yes | 1 to 80 characters. |
| `description` | no | 1 to 500 characters. |
| `tools` | no | Up to 16 tool names; the server allows `web_search` and `web_fetch`. |
| `[[trackers]]` | no | Up to 12, unique `name`. Same shape as a tracker row: `fields` (1 to 20: `key`, `label`, `kind` of number, text, bool or choice, `unit`, `choices`, and an optional Apple Health `health` source: `workout_type` fills a text field, `body_mass`, `workout_minutes`, `workout_energy`, `steps` and `dietary_energy` fill number fields), optional `cadence` (`freq`, `weekdays`, `at`) and `rules`. A number field may add `target`. |
| `[[personas]]` | no | Up to 8: `name` (slug) and `toml` (a persona file, checked by the persona parser on the server). |
| `[[sections]]` | yes | 1 to 12, unique `title`, each with 1 to 24 `[[sections.items]]`. |

The whole bundle is at most 64 KiB. Lists inside items hold at most 50 entries and texts at most 500 characters.

## Items

An item is a table with `kind` and exactly that kind's props from `apps/aroh/src/lib/ui/contract.ts`, the one source of truth. `aroh_plugins::contract::KINDS` mirrors it and a test fails on any drift. A prop is a literal of the contract type, or a binding. A left-out nullable prop (such as `number_input.value`) is null. `question_card.input` is a nested item, never another question card. `photo_timeline.photos` takes only an empty list, since a bundle must not carry image URLs.

## Bindings

A binding is `{ bind = "<source>", ... }`. Its source's type must equal the prop's type.

| Source | Keys | Gives |
|---|---|---|
| `series` | `tracker`, `field` | `Point[]`: the field's entries, oldest first |
| `latest` | `tracker`, `field` | `number`: the newest value |
| `today_total` | `tracker`, `field` | `number`: the sum of today's values |
| `change` | `tracker`, `field` | `number`: newest minus oldest value of the last 7 days, matching the stat tile's "this week" |
| `target` | `tracker`, `field` | `number`: the field's declared `target` |
| `entries` | `tracker` | log list entries, newest 20 |
| `streak` | `tracker` (optional) | 28 heatmap days: logged days of a tracker, or done actions of the goal |
| `actions` | `when` (`next`, `today`, `week`), `limit` | `ActionView[]` of the goal |
| `week_done`, `week_planned` | none | `number`: this week's done and planned actions |
| `week_start` | none | `string`: this week's Monday |
| `deadline` | none | `string`: the goal's deadline instant |

Field sources need a number field. When a binding has no data (no entries, no deadline, nothing planned this week), its item is left out, and a section left with no items is left out too.

## Templates

`crates/aroh-plugins/templates/` ships the built-in bundles. `fitness` reproduces the derived dashboard for its Weight, Protein, Workouts and Steps trackers: countdown, this week's ring, one line per tracker with data, the four-week streak and the next four sessions. Weight, workouts and steps declare their Apple Health sources.

| Template | Trackers | View |
|---|---|---|
| `music` | Practice (minutes), Tempo (bpm), Pieces (piece, progress %) | countdown, week ring, minutes bar chart, tempo line, practice streak, piece progress tile, practice notes, next sessions |
| `learning` | Sessions (minutes), Modules (done), Quizzes (score %) | countdown, week ring, modules tile with weekly change, minutes bar chart, quiz line, study streak, next sessions |
| `finance` | Savings (amount with a target), Spending (amount, what for) | savings vs target, savings line, spent today, spending bar chart and log, next money tasks |
| `habits` | Check-in (done, note) | today's checks, week ring, check-in and action streaks, notes |

Units outside the contract's list (bpm, percent) chart as `count` with the unit in the label. The server gives the plugin builder the template of the goal's team (`crates/aroh-server/src/teams.rs`): fitness for health and fitness, music for music goals in Hobby and Skill, learning, finance, and habits for habit and general goals. Owners can reuse any template with `plugins_use_template`.
