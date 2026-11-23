# Pod 25: build 1 quick fixes (TestFlight build 2)

**Goal:** fix what the owner hit on TestFlight build 1 (Q58), as decided in Q59 and Q60. Branch `fix/build1-quick-fixes`.

**Process:** PM plans, one engineer writes tests first (run red first), then code. Tests run in Kiri. Simulator `aroh-p25`, dev port 1431 (never committed), screenshots in `docs/screenshots/pod25/`, `git checkout -- apps/aroh/src-tauri/gen/apple` afterwards.

## Stuck answer: evidence and root-cause hypotheses

Production: one `new-goal` run, `waiting_human` at `questions`, one job (done), no answer job after the owner answered three question cards. A posted answer always enqueues a job (`Engine::answer` in `crates/aroh-server/src/engine.rs`), so no answer reached the server.

The app holds answers in memory and posts only when the held count reaches the question count (`route` in `apps/aroh/src/lib/app/engine.ts`, `GoalChat.send` in `chat.svelte.ts`). Hypotheses, to confirm by a red test and a simulator repro before the fix:

1. Lost update: `PersonaChat.handle` fires `void this.send(text)` per card, and `send` passes `this.held` into `sendChat`, which does two engine round trips before `held` is written back. Cards answered while an earlier send is in flight all start from the same `held`, so the count never reaches 3.
2. Silent drop: `QuestionCard` shows "Answer sent" and emits `undefined` when its input never changed; `PersonaChat.answer` drops an empty answer with no message, so the owner sees sent but nothing was held.
3. Held answers live only in the `GoalChat` instance; leaving the goal screen loses them.

Fix at the root: no holding. The questions of a run render as one group with one Submit that posts every answer in one `POST /v1/runs/{id}/answers` and reports an error in the chat.

## Scope

| # | Item | Notes |
|---|---|---|
| 1 | Phone UI | `app.html` viewport `maximum-scale=1, user-scalable=no`; every input, textarea and select at least 16 px; keyboard: research the WKWebView approach (content inset vs `interactive-widget`, `visualViewport` resize) so the focused input and the conversation stay visible without layout jumps. |
| 2 | Built-in defaults | One config place for `supabase_url` `https://dgxkjlgsxxejpapsdzjd.supabase.co`, `supabase_anon_key` (publishable key from the leader), `server_url` `https://aroh-ten.vercel.app`; used when the setting is empty; still editable in Settings. |
| 3 | First-launch setup | From existing components: notifications, Calendar, Health, each with a one-line reason, skippable, shown once; then sign in. |
| 4 | Questions | One group, one Submit, one structured POST, typed text still works, errors shown; open question sets reappear on goal open (fetch waiting runs on load); number questions carry `unit`, `min`, `max`, typing plus slider, never "count"; Inbox and goal status say what is waited for ("3 questions to answer"), tap opens them. |
| 5 | Intake quality | Research before asking (existing research and search tools), ask only what research cannot find, in the owner's terms, never ask the owner to compute; `unit`, `min`, `max` on number questions in the typed output; evals and tests updated. |
| 6 | Run limits (Q60) | Active-work limit per run, default 5 min, stored per user, editable in Settings; `waiting_human` and `waiting_budget` time excluded; on the limit the run stops with no more model calls, the app shows "Stopped after 5 min" with Retry; while working the goal shows the running step, cost so far and Stop (cancel endpoint). Server tests for limit and cancel. |
| 7 | Mic and attach | Composer mic reuses the trackers voice input (`/log`) with mic and speech usage strings in `Info.plist` and `project.yml`, else hidden; attach hidden until Q61. |

## Acceptance

Local e2e stack (own Kiri project and ports), fake script branch for "Finish reading <book> by next Sunday": create goal, research-first questions with units, submit all answers once, plan arrives. Screenshots for each step and for the keyboard open. Supabase migration only if needed, listed in DONE.
