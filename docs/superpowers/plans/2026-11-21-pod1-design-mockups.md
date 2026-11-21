# Pod 1: Design Tokens, Component Library and Screen Mockups

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Svelte mockup of the design tokens, the four component families and six key screens, built from real Svelte code in the app and published as one artifact for the owner's approval.

**Architecture:** Tokens live once in `src/lib/ui/tokens/tokens.css`. shadcn-svelte primitives (copied as source by its CLI) sit in `src/lib/ui/primitives/`. Aroh components sit in `src/lib/ui/components/<family>/` and take props typed in one contract file, `src/lib/ui/contract.ts`; that contract later becomes the plugin view schema. Screen mockups compose components with sample data from one file. A single prerendered route `/mockups` switches views with component state (no URL routing, because the artifact allows only `#name` hashes) and is exported as one HTML file.

**Tech Stack:** Svelte 5 runes, SvelteKit 3 adapter-static, TypeScript, Tailwind CSS v4, shadcn-svelte (Bits UI), shadcn-svelte chart (LayerChart) or plain SVG.

## Global Constraints

- Visual direction: simple, professional, enterprise grade, intuitive; calendar-first like a plain web calendar with cards on top; neutral palette, one accent (`--accent`), no gradients, no heavy shadows, no decorative color.
- Every color, font, radius and spacing value comes from `tokens.css`. No literal colors in components.
- Light and dark themes both work.
- Phone width 390 px is the primary target; desktop 1280 px second.
- Touch targets at least 44 px on phone.
- No code comments; docstrings only above declarations. No em dashes in copy or code.
- One copy of every helper: shared formatting lives in `src/lib/ui/format.ts`, sample data in `src/lib/ui/mockdata/sixpack.ts`.
- Mockup code is the code that gets ported later: write it as production components, not throwaway markup.
- Work on branch `feat/design-mockups` in its own worktree.
- Adding npm dependencies `tailwindcss`, `@tailwindcss/vite`, `bits-ui`, `tailwind-variants`, `clsx`, `tailwind-merge`, `layerchart` and the shadcn-svelte CLI output is approved (decision Q18). Any other dependency: ask the leader first.

---

## File Structure

| Path | Responsibility |
|---|---|
| `apps/aroh/src/lib/ui/tokens/tokens.css` | all design tokens, light and dark |
| `apps/aroh/src/app.css` | imports Tailwind and tokens, maps tokens into the Tailwind theme |
| `apps/aroh/src/lib/ui/primitives/*` | shadcn-svelte generated primitives |
| `apps/aroh/src/lib/ui/contract.ts` | typed props for every Aroh component, renderer-neutral |
| `apps/aroh/src/lib/ui/format.ts` | number, unit, date and duration formatting |
| `apps/aroh/src/lib/ui/components/inputs/*.svelte` | input family |
| `apps/aroh/src/lib/ui/components/display/*.svelte` | data display family |
| `apps/aroh/src/lib/ui/components/planning/*.svelte` | planning family |
| `apps/aroh/src/lib/ui/components/agent/*.svelte` | agent surfaces family |
| `apps/aroh/src/lib/ui/shell/*.svelte` | app shell: phone tab bar, desktop sidebar |
| `apps/aroh/src/lib/ui/mockdata/sixpack.ts` | one sample goal used by every mockup |
| `apps/aroh/src/lib/ui/screens/*.svelte` | Today, Calendar, Goal, NewGoal, Inbox, Settings screens |
| `apps/aroh/src/routes/mockups/+page.svelte` | view switcher: tokens, components, screens |
| `apps/aroh/src/routes/mockups/+page.ts` | `export const prerender = true` |

---

### Task 1: Tailwind, shadcn-svelte and tokens

**Files:**
- Create: `apps/aroh/src/lib/ui/tokens/tokens.css`
- Modify: `apps/aroh/src/app.css`, `apps/aroh/vite.config.ts`, `apps/aroh/package.json`
- Create: `apps/aroh/components.json` (shadcn-svelte CLI)

**Interfaces:**
- Produces: CSS custom properties below, available as Tailwind colors `bg`, `surface`, `surface-2`, `border`, `fg`, `muted`, `accent`, `accent-soft`, `ok`, `warn`, `danger`, `cat-1` to `cat-6`.

- [ ] **Step 1: Install Tailwind v4 and the Vite plugin**

Run in `apps/aroh`: `pnpm add -D tailwindcss @tailwindcss/vite`
Expected: both appear in `devDependencies`.

- [ ] **Step 2: Add the plugin to `vite.config.ts`**

```ts
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

const host = process.env.TAURI_DEV_HOST;
const mockup = process.env.AROH_MOCKUP === '1';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			adapter: adapter({ fallback: 'index.html' }),
			output: { bundleStrategy: mockup ? 'inline' : 'split' }
		})
	],
	resolve: { alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) } },
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true,
		host: host || false,
		hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
		watch: { ignored: ['**/src-tauri/**'] }
	}
});
```

If the installed SvelteKit rejects `output` inside the plugin options, move it to `kit.output` wherever this project's SvelteKit 3 config reads kit options, and note the location in the DONE report.

- [ ] **Step 3: Write `tokens.css`**

```css
:root {
	--bg: #ffffff;
	--surface: #f7f8fa;
	--surface-2: #eef1f5;
	--border: #dce0e6;
	--fg: #1f2329;
	--muted: #5d6572;
	--accent: #2f63d6;
	--accent-fg: #ffffff;
	--accent-soft: #e8effc;
	--ok: #1e7f4f;
	--warn: #9a6400;
	--danger: #b3261e;
	--cat-1: #3b6fd8;
	--cat-2: #1f8a70;
	--cat-3: #b26a00;
	--cat-4: #8a4fbf;
	--cat-5: #c0392b;
	--cat-6: #5b6b7f;
	--font: 'Inter', system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
	--font-mono: 'JetBrains Mono', ui-monospace, 'SF Mono', Menlo, monospace;
	--text-xs: 12px;
	--text-sm: 13px;
	--text-md: 15px;
	--text-lg: 17px;
	--text-xl: 20px;
	--text-2xl: 24px;
	--space-1: 4px;
	--space-2: 8px;
	--space-3: 12px;
	--space-4: 16px;
	--space-5: 24px;
	--space-6: 32px;
	--radius-sm: 6px;
	--radius-md: 10px;
	--radius-full: 999px;
	--touch: 44px;
	--tabbar: 56px;
	--sidebar: 248px;
	--shadow-card: 0 1px 2px rgb(16 24 40 / 0.06);
}
@media (prefers-color-scheme: dark) {
	:root:not([data-theme='light']) {
		--bg: #15181d; --surface: #1b1f25; --surface-2: #232830; --border: #343a44;
		--fg: #e7eaef; --muted: #9aa2b1; --accent: #7aa2f7; --accent-fg: #0f1420; --accent-soft: #1e2a44;
		--ok: #5fc792; --warn: #e0b154; --danger: #f2827a;
		--cat-1: #7aa2f7; --cat-2: #5cc8a8; --cat-3: #e0a85a; --cat-4: #b993e6; --cat-5: #ef8a7e; --cat-6: #9aa8ba;
		--shadow-card: none; color-scheme: dark;
	}
}
:root[data-theme='dark'] {
	--bg: #15181d; --surface: #1b1f25; --surface-2: #232830; --border: #343a44;
	--fg: #e7eaef; --muted: #9aa2b1; --accent: #7aa2f7; --accent-fg: #0f1420; --accent-soft: #1e2a44;
	--ok: #5fc792; --warn: #e0b154; --danger: #f2827a;
	--cat-1: #7aa2f7; --cat-2: #5cc8a8; --cat-3: #e0a85a; --cat-4: #b993e6; --cat-5: #ef8a7e; --cat-6: #9aa8ba;
	--shadow-card: none; color-scheme: dark;
}
```

- [ ] **Step 4: Replace `app.css` with the Tailwind entry**

```css
@import 'tailwindcss';
@import './lib/ui/tokens/tokens.css';

@theme inline {
	--color-bg: var(--bg);
	--color-surface: var(--surface);
	--color-surface-2: var(--surface-2);
	--color-border: var(--border);
	--color-fg: var(--fg);
	--color-muted: var(--muted);
	--color-accent: var(--accent);
	--color-accent-fg: var(--accent-fg);
	--color-accent-soft: var(--accent-soft);
	--color-ok: var(--ok);
	--color-warn: var(--warn);
	--color-danger: var(--danger);
	--color-cat-1: var(--cat-1);
	--color-cat-2: var(--cat-2);
	--color-cat-3: var(--cat-3);
	--color-cat-4: var(--cat-4);
	--color-cat-5: var(--cat-5);
	--color-cat-6: var(--cat-6);
	--font-sans: var(--font);
	--font-mono: var(--font-mono);
	--radius-sm: var(--radius-sm);
	--radius-md: var(--radius-md);
}

html, body { background: var(--bg); color: var(--fg); font-family: var(--font); font-size: var(--text-md); }
```

The old screens still use old variable names (`--surface-raised`, `--ink`, `--line`). Keep them working by adding, at the end of `tokens.css`, `:root { --surface-raised: var(--bg); --ink: var(--fg); --ink-muted: var(--muted); --line: var(--border); --accent-ink: var(--accent-fg); }`. Pod 10 deletes the old screens and these aliases together.

- [ ] **Step 5: Initialise shadcn-svelte into `src/lib/ui/primitives`**

Run in `apps/aroh`: `pnpm dlx shadcn-svelte@latest init`, choosing base color "slate", CSS file `src/app.css`, components alias `$lib/ui/primitives`, utils alias `$lib/ui/utils`. Then re-apply Step 4's `@theme inline` block if the CLI rewrote `app.css`, keeping the CLI's own variables mapped onto our tokens (`--background: var(--bg)`, `--foreground: var(--fg)`, `--primary: var(--accent)`, `--primary-foreground: var(--accent-fg)`, `--border: var(--border)`, `--muted-foreground: var(--muted)`, `--card: var(--bg)`, `--popover: var(--bg)`, `--ring: var(--accent)`).

- [ ] **Step 6: Verify**

Run: `pnpm check`
Expected: 0 errors.

- [ ] **Step 7: Commit**

```bash
git add apps/aroh
git commit -m "feat(ui): tailwind, shadcn-svelte and design tokens"
```

### Task 2: Primitives

**Files:**
- Create: `apps/aroh/src/lib/ui/primitives/*` (CLI output)

- [ ] **Step 1: Add primitives**

Run in `apps/aroh`: `pnpm dlx shadcn-svelte@latest add button card input label textarea select checkbox switch radio-group slider tabs badge dialog sheet popover tooltip separator scroll-area toggle-group progress avatar chart`
Expected: folders appear under `src/lib/ui/primitives/`.

- [ ] **Step 2: Verify**

Run: `pnpm check`
Expected: 0 errors. If `chart` fails to install, drop it from the command, note it in DONE, and draw charts with plain SVG in Task 4.

- [ ] **Step 3: Commit**

```bash
git add apps/aroh
git commit -m "feat(ui): add shadcn-svelte primitives"
```

### Task 3: Component contract, format helpers and sample data

**Files:**
- Create: `apps/aroh/src/lib/ui/contract.ts`, `apps/aroh/src/lib/ui/format.ts`, `apps/aroh/src/lib/ui/mockdata/sixpack.ts`
- Test: `apps/aroh/src/lib/ui/format.test.ts`

**Interfaces:**
- Produces: `contract.ts` types below; `format.ts` exports `formatNumber(value: number, unit?: string): string`, `formatDuration(minutes: number): string`, `formatTime(iso: string, zone: string): string`, `formatDay(iso: string, zone: string): string`, `daysUntil(iso: string, nowIso: string): number`; `sixpack.ts` exports `sixpackGoal`, `sixpackActions`, `sixpackEntries`, `sixpackInbox`, `sixpackChat`, `providerCatalog`.

- [ ] **Step 1: Write `contract.ts`**

```ts
/** Tracker units understood by inputs and charts. */
export type Unit = 'kg' | 'lb' | 'cm' | 'kcal' | 'g' | 'ml' | 'L' | 'min' | 'reps' | 'steps' | 'usd' | 'count';

/** One category colour slot, 1 to 6, mapped to --cat-N. */
export type CategorySlot = 1 | 2 | 3 | 4 | 5 | 6;

/** One point of a tracker series. */
export type Point = { at: string; value: number };

/** One planned or past action as the UI shows it. */
export type ActionView = {
	id: string;
	title: string;
	start: string;
	end: string;
	status: 'planned' | 'done' | 'missed' | 'skipped';
	locked: boolean;
	category: CategorySlot;
	goalTitle: string;
};

/** One change inside an agent proposal. */
export type ChangeView = {
	op: 'move' | 'resize' | 'create' | 'delete';
	title: string;
	before: string | null;
	after: string | null;
};

/** Renderer-neutral component spec: the shape plugin view trees use. */
export type ComponentSpec =
	| { kind: 'number_input'; label: string; unit: Unit; value: number | null; step: number; min?: number; max?: number }
	| { kind: 'rating'; label: string; min: number; max: number; value: number | null }
	| { kind: 'choice'; label: string; options: string[]; multi: boolean; value: string[] }
	| { kind: 'note'; label: string; placeholder: string; value: string }
	| { kind: 'check_off'; label: string; done: boolean }
	| { kind: 'counter'; label: string; unit: Unit; value: number; target: number }
	| { kind: 'timer'; label: string; targetMinutes: number }
	| { kind: 'capture'; label: string; media: 'photo' | 'audio' | 'video' }
	| { kind: 'line_chart'; label: string; unit: Unit; points: Point[]; target?: number }
	| { kind: 'bar_chart'; label: string; unit: Unit; points: Point[]; target?: number }
	| { kind: 'progress_ring'; label: string; unit: Unit; value: number; target: number }
	| { kind: 'target_vs_actual'; label: string; unit: Unit; target: number; actual: number }
	| { kind: 'streak_heatmap'; label: string; days: { date: string; level: 0 | 1 | 2 | 3 }[] }
	| { kind: 'stat_tile'; label: string; unit: Unit; value: number; delta?: number }
	| { kind: 'log_list'; label: string; entries: { at: string; text: string; value?: number; unit?: Unit }[] }
	| { kind: 'photo_timeline'; label: string; photos: { at: string; src: string }[] }
	| { kind: 'action_list'; label: string; actions: ActionView[] }
	| { kind: 'week_strip'; label: string; actions: ActionView[]; startDay: string }
	| { kind: 'milestone_timeline'; label: string; milestones: { title: string; due: string; done: boolean }[] }
	| { kind: 'module_checklist'; label: string; modules: { title: string; minutes: number; done: boolean }[] }
	| { kind: 'countdown'; label: string; deadline: string }
	| { kind: 'insight_card'; persona: string; title: string; body: string }
	| { kind: 'proposal_card'; persona: string; summary: string; changes: ChangeView[] }
	| { kind: 'question_card'; persona: string; question: string; input: ComponentSpec };

/** A plugin view: a titled column of component specs, phone first. */
export type ViewSpec = { title: string; sections: { title: string; items: ComponentSpec[] }[] };
```

- [ ] **Step 2: Write the failing test for `format.ts`**

```ts
import { describe, expect, it } from 'vitest';
import { daysUntil, formatDuration, formatNumber, formatTime } from './format';

describe('format', () => {
	it('formats numbers with units', () => {
		expect(formatNumber(82.4, 'kg')).toBe('82.4 kg');
		expect(formatNumber(2400, 'kcal')).toBe('2,400 kcal');
		expect(formatNumber(3)).toBe('3');
	});
	it('formats durations', () => {
		expect(formatDuration(45)).toBe('45 min');
		expect(formatDuration(90)).toBe('1 h 30 min');
		expect(formatDuration(120)).toBe('2 h');
	});
	it('formats a time in a zone', () => {
		expect(formatTime('2026-11-21T18:00:00Z', 'UTC')).toBe('18:00');
	});
	it('counts whole days to a deadline', () => {
		expect(daysUntil('2027-02-01T00:00:00Z', '2026-11-21T00:00:00Z')).toBe(72);
	});
});
```

- [ ] **Step 3: Add vitest and run the test to see it fail**

Run in `apps/aroh`: `pnpm add -D vitest` then `pnpm vitest run src/lib/ui/format.test.ts`
Expected: FAIL, module `./format` not found.

- [ ] **Step 4: Write `format.ts`**

```ts
const MS_PER_DAY = 86_400_000;

/** Formats a number with thousands separators and an optional unit, for example "2,400 kcal". */
export function formatNumber(value: number, unit?: string): string {
	const text = new Intl.NumberFormat('en-US', { maximumFractionDigits: 1 }).format(value);
	return unit ? `${text} ${unit}` : text;
}

/** Formats minutes as "45 min", "1 h 30 min" or "2 h". Input must be a non-negative integer. */
export function formatDuration(minutes: number): string {
	if (!Number.isInteger(minutes) || minutes < 0) throw new RangeError(`bad minutes ${minutes}`);
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	if (h === 0) return `${m} min`;
	return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** Formats an ISO instant as 24-hour "HH:MM" in an IANA zone. */
export function formatTime(iso: string, zone: string): string {
	return new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', timeZone: zone }).format(new Date(iso));
}

/** Formats an ISO instant as "Sat 21 Nov" in an IANA zone. */
export function formatDay(iso: string, zone: string): string {
	return new Intl.DateTimeFormat('en-GB', { weekday: 'short', day: 'numeric', month: 'short', timeZone: zone }).format(new Date(iso));
}

/** Whole days from `nowIso` to `iso`, rounded down, never negative. */
export function daysUntil(iso: string, nowIso: string): number {
	const diff = Date.parse(iso) - Date.parse(nowIso);
	return Math.max(0, Math.floor(diff / MS_PER_DAY));
}
```

- [ ] **Step 5: Run the test**

Run: `pnpm vitest run src/lib/ui/format.test.ts`
Expected: 4 passed.

- [ ] **Step 6: Write `mockdata/sixpack.ts`**

Sample data for one goal, typed with `contract.ts`, used by every screen and the gallery:
- `sixpackGoal`: title "Six pack by Feb 1", category "Health and Fitness" (slot 2), deadline `2027-02-01`, autonomy "Balanced", personas ["Fitness coach", "Nutritionist", "Planner"], tracker targets: weight 78 kg (now 82.4), body fat 12 percent (now 18), protein 150 g per day, workouts 4 per week.
- `sixpackActions`: 14 days of `ActionView` from 2026-11-21: strength sessions Mon, Wed, Fri 18:00 to 19:00; core sessions Tue, Thu 07:00 to 07:30; a Sunday 30 min meal prep; one missed Wednesday session; plus two busy blocks from other goals ("Learn piano" slot 4 practice 20:00 to 20:30 daily, "Save $10k" slot 3 Friday 12:00 review).
- `sixpackEntries`: 21 daily weight points from 84.0 down to 82.4 with noise, 7 days of protein grams (118 to 162), meal log entries ("Chicken rice bowl, 42 g protein").
- `sixpackInbox`: one `proposal_card` (Fitness coach: "Move Wednesday's missed strength session to Thursday 18:00 and shorten Thursday core to 20 min", 2 changes), one `insight_card` (Nutritionist: "Protein below 150 g on 4 of the last 7 days"), one preference suggestion ("You moved workouts to evenings 3 times. Plan them 18:00 to 20:00 from now on?"), one `question_card` (Fitness coach asks today's weight, `number_input` kg).
- `sixpackChat`: the New Goal conversation: owner message "I want a six pack by Feb 1", orchestrator reply, three question cards (weight and height, current diet choice, training days weekday picker as `choice` multi), research progress lines ("Reading 3 sources on fat loss rates", "Fitness coach drafting a 10-week split"), final plan summary with an Approve button.
- `providerCatalog`: Anthropic (claude-opus-5-5, claude-sonnet-5-5, claude-haiku-5-5), OpenAI, Google Gemini, OpenRouter, Groq, Ollama (self-hosted), each with protocol and a "key set" flag.

- [ ] **Step 7: Commit**

```bash
git add apps/aroh
git commit -m "feat(ui): component contract, format helpers and sample goal data"
```

### Task 4: Component families

**Files:**
- Create one `.svelte` file per `ComponentSpec.kind`, named in PascalCase, in its family folder:
  - `components/inputs/`: `NumberInput`, `Rating`, `Choice`, `Note`, `CheckOff`, `Counter`, `Timer`, `Capture`
  - `components/display/`: `LineChart`, `BarChart`, `ProgressRing`, `TargetVsActual`, `StreakHeatmap`, `StatTile`, `LogList`, `PhotoTimeline`
  - `components/planning/`: `ActionList`, `WeekStrip`, `MilestoneTimeline`, `ModuleChecklist`, `Countdown`
  - `components/agent/`: `InsightCard`, `ProposalCard`, `QuestionCard`, `PersonaChat`
- Create: `apps/aroh/src/lib/ui/components/Render.svelte`

**Interfaces:**
- Consumes: `contract.ts`, `format.ts`, primitives.
- Produces: each component takes exactly its spec as props (`let { spec }: { spec: Extract<ComponentSpec, { kind: 'number_input' }> } = $props();`) and emits changes through one optional `onchange: (value: unknown) => void` prop. `Render.svelte` takes `{ spec: ComponentSpec }` and switches on `spec.kind` to the right component; it is the only place that maps kinds to components.

Rules for every component:
- Built from primitives where one exists (Button, Card, Input, Slider, Checkbox, Badge, Progress).
- Visible label, keyboard focus ring, 44 px touch targets, works at 390 px.
- States shown in the gallery: empty, filled, and (where it applies) over-target or missed.
- Charts: thin lines, faint horizontal grid, dashed target line, last point emphasised, axis labels in `--muted`.
- Category colour only as a 3 px left edge or a dot on action items, never as a full fill.

- [ ] **Step 1: Inputs family** (8 files), then `pnpm check`, expected 0 errors, then commit `feat(ui): input components`.
- [ ] **Step 2: Data display family** (8 files), then `pnpm check`, then commit `feat(ui): data display components`.
- [ ] **Step 3: Planning family** (5 files), then `pnpm check`, then commit `feat(ui): planning components`.
- [ ] **Step 4: Agent surfaces family** (4 files): `ProposalCard` has Apply and Reject buttons and lists each change as "before -> after"; `QuestionCard` renders its `input` through `Render.svelte`; `PersonaChat` shows owner and persona messages, a composer with text field, attach and mic buttons. Then `pnpm check`, then commit `feat(ui): agent surface components`.
- [ ] **Step 5: `Render.svelte`**, then `pnpm check`, then commit `feat(ui): component renderer`.

### Task 5: Shell and screens

**Files:**
- Create: `apps/aroh/src/lib/ui/shell/PhoneShell.svelte`, `apps/aroh/src/lib/ui/shell/DesktopShell.svelte`
- Create: `apps/aroh/src/lib/ui/screens/TodayScreen.svelte`, `CalendarScreen.svelte`, `GoalScreen.svelte`, `NewGoalScreen.svelte`, `InboxScreen.svelte`, `SettingsScreen.svelte`

**Interfaces:**
- Consumes: components, `Render.svelte`, `mockdata/sixpack.ts`.
- Produces: each screen takes no props in the mockup and reads sample data; pod 10 swaps sample data for the typed API client.

Screen content:
- **Shell:** phone: top bar (screen title, avatar opens Settings), bottom tab bar Today, Calendar, Goals, Inbox (badge with count), Log. Desktop: left sidebar with the same five items plus a category filter list with colour dots, main area.
- **Today:** date header, next action card, check-in question card (weight), today's actions list with done buttons, one insight card, quick-log bar at the bottom (text field, mic, photo).
- **Calendar:** desktop week view with an hour grid 06:00 to 22:00, events as light cards with a category left edge, today column tinted `--accent-soft`, header with Today, previous and next, Day, Week, Month switch, and category filter chips. Phone: day view with the same events and a week strip on top.
- **Goal:** six-pack dashboard from a `ViewSpec`: countdown, weight line chart with target, protein bar chart with target, workouts progress ring, streak heatmap, upcoming actions, chat-with-personas entry, autonomy level selector (Ask, Balanced, Auto).
- **New Goal:** `PersonaChat` with the sample conversation, question cards answered inline, live research progress, plan summary with Approve and Change buttons.
- **Inbox:** grouped list: Needs your approval (proposal card), Suggestions (preference card), Insights, Runs in progress (progress line).
- **Settings:** Model provider section: provider dropdown, model dropdown, API key field (password, "Saved on the server, never on this device" helper text), Test connection button; Search providers list with add button; Server URL field; Monthly budget fields; Calendars and Apple Health toggles.

- [ ] **Step 1: Shells**, then `pnpm check`, then commit `feat(ui): phone and desktop shells`.
- [ ] **Step 2: Today and Calendar**, then `pnpm check`, then commit `feat(ui): today and calendar mockups`.
- [ ] **Step 3: Goal and New Goal**, then `pnpm check`, then commit `feat(ui): goal and new goal mockups`.
- [ ] **Step 4: Inbox and Settings**, then `pnpm check`, then commit `feat(ui): inbox and settings mockups`.

### Task 6: Mockup page and export

**Files:**
- Create: `apps/aroh/src/routes/mockups/+page.svelte`, `apps/aroh/src/routes/mockups/+page.ts`

- [ ] **Step 1: Write `+page.ts`**

```ts
export const prerender = true;
export const ssr = true;
```

- [ ] **Step 2: Write `+page.svelte`**

A top bar with a segmented control: Tokens, Inputs, Display, Planning, Agent, Screens (phone), Screens (desktop), and a light/dark toggle that sets `data-theme` on `document.documentElement`. State is a `$state` variable; the selected view also writes `location.hash` with a plain name (`#tokens`, `#inputs`, `#screens-phone`) and reads it on load, so links can point at one view. Phone screens render side by side, each inside a 390 x 844 frame with a 1 px border and the screen name under it. Desktop screens render at 1280 px wide inside a scroll container. Tokens view shows colour swatches with names and values, the type scale, spacing and radius samples. Component views render every gallery state through `Render.svelte` with a caption naming the `kind`.

- [ ] **Step 3: Build the single-file mockup**

Run in `apps/aroh`: `AROH_MOCKUP=1 pnpm build`
Expected: `build/mockups.html` (or `build/mockups/index.html`) exists and contains inline `<script>` and `<style>` with no `_app/` references: `grep -c '_app/' build/mockups.html` prints `0`.
If the inline strategy is unavailable, build normally and record that the artifact must be published with `files` for `build/_app/**`.

- [ ] **Step 4: Smoke check in a browser**

Run: `pnpm preview --port 4173` and open `http://localhost:4173/mockups`. Check every view at 390 px and 1280 px in both themes. Fix any overflow, clipped text or unreadable colour.

- [ ] **Step 5: Commit and report DONE**

```bash
git add apps/aroh
git commit -m "feat(ui): mockup gallery page"
```

DONE line: `DONE feat/design-mockups <base>..<tip> VERIFIED build/mockups.html`. The leader publishes `build/mockups.html` as the "Aroh Mockups" artifact and asks the owner to approve.
