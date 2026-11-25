<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import GoalCard from '$lib/ui/components/goal/GoalCard.svelte';
	import RoutinesScreen from '$lib/ui/screens/RoutinesScreen.svelte';
	import * as location from '$lib/app/location';
	import { onMount, untrack } from 'svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { goalHeader, goalView } from '$lib/app/views';
	import { engine, progressLines } from '$lib/app/engine';
	import { GoalChat } from '$lib/app/chat.svelte';
	import { answersReason, chipFor, goalStatus, markRead, planWaits, runState, settledMark } from '$lib/app/goalcard';
	import { dashboard, type Pace } from '$lib/app/dashboard';
	import type { AgentSettings } from '$lib/app/newgoal';
	import { speech } from '$lib/app/speech.svelte';
	import { activeVersion, enablePlugin, listPlugins, type PluginVersion } from '$lib/plugins/api';
	import { parseTree, resolve } from '$lib/plugins/resolve';
	import { pluginPageFor } from '$lib/plugins/describe';
	import type { CoachView } from '$lib/ui/screens/types';
	import { coachesOn, loadCoaches, pluginsUsed, type GoalRun, type PersonaView } from '$lib/app/personas';
	import { goalAttachments, pendingAttachment, uploadFailed } from '$lib/app/files';
	import type { Attachment } from '$lib/ui/components/goal/Attachments.svelte';
	import DeleteGoalSheet from '$lib/ui/components/goal/DeleteGoalSheet.svelte';
	import RunAgainSheet from '$lib/ui/components/goal/RunAgainSheet.svelte';
	import type { EngineResult, RunAnswer, RunSummary } from '$lib/app/engine';
	import type { PlanContext } from '$lib/ui/components/plan/PlanCard.svelte';
	import { AHEAD_DAYS } from '$lib/app/calendar';
	import { forGoal, openGoalId, tagged, type Tagged } from '$lib/app/goalpage';
	import { formatYmd, shiftDay } from '$lib/ui/format';

	const DERIVED = 'derived';
	const SHOWN_VERSIONS = 3;
	const TICK_MS = 1000;
	const DEFAULT_LIMIT = 5;

	const clock = getClock();
	const s = $derived(data.snapshot);
	const goal = $derived(s?.goals.find((g) => g.id === page.params.id));
	const openId = $derived(openGoalId(s, page.params.id));
	let pluginRows = $state<Tagged<PluginVersion[]> | null>(null);
	const plugins = $derived(forGoal(pluginRows, goal?.id, []));
	let places = $state<location.LocationView | null>(null);
	const routines = $derived(!!goal && !!places && goal.id === places.routines_goal);
	let helpers = $state<Tagged<{ library: PersonaView[]; runs: GoalRun[] }> | null>(null);
	const mine = $derived(forGoal(helpers, goal?.id, null));
	const helpersFor = $derived(mine ? (goal?.id ?? null) : null);
	const goalRuns = $derived(mine?.runs ?? []);
	let settingsLimit = $state(engine.cachedRunLimit()?.minutes ?? DEFAULT_LIMIT);
	let now = $state(Date.now());
	let upload = $state(false);
	onMount(async () => (upload = await client.filesAvailable()));

	let failed = $state<Attachment[]>([]);
	const retries = new Map<string, File>();

	/** Uploads `list` one by one; a failed one stays listed with Retry and keeps its file. */
	async function uploadFiles(list: File[]) {
		const id = goal?.id;
		if (!id) return;
		for (const file of list) {
			const key = `u${retries.size}-${file.name}`;
			try {
				await client.attachFile(id, file);
			} catch (e) {
				retries.set(key, file);
				failed = [...failed, uploadFailed(pendingAttachment(key, file))];
				data.error = `${file.name}: ${String(e)}`;
			}
		}
		await data.load();
	}

	/** Sends a failed upload again, keeping it listed if it fails again. */
	async function retryUpload(key: string) {
		const file = retries.get(key);
		const id = goal?.id;
		if (!file || !id) return;
		try {
			await client.attachFile(id, file);
			retries.delete(key);
			failed = failed.filter((f) => f.id !== key);
			data.error = null;
			await data.load();
		} catch (e) {
			data.error = `${file.name}: ${String(e)}`;
		}
	}

	/** Runs one attachment write, then reloads the snapshot; a refusal shows as the screen error. */
	function attaching(write: () => Promise<unknown>) {
		void data.run(async () => {
			await write();
			await data.load();
		});
	}

	onMount(() => location.follow((v) => (places = v), (e) => (data.error = String(e))));
	onMount(() => {
		void engine.runLimit().then((r) => r.ok && (settingsLimit = r.value.minutes));
		const timer = setInterval(() => (now = Date.now()), TICK_MS);
		return () => clearInterval(timer);
	});

	let opened: Tagged<Promise<EngineResult<RunSummary[]>>> | null = null;
	let coachesFor: string | null = null;
	let coachesRead: string | null = null;

	/** The runs of goal `id`, read once per open and shared by the chat and the coaches (B87). */
	function runsOnOpen(id: string): Promise<EngineResult<RunSummary[]>> {
		if (opened?.id !== id) opened = tagged(id, engine.runs({ goalId: id }));
		return opened.value;
	}

	const chat = $derived(new GoalChat(openId, 'Tell the agents anything about this goal. They read it at their next step.'));
	$effect(() => void chat.open(chat.goalId ? runsOnOpen(chat.goalId) : undefined));

	const ready = $derived(!!s);
	let runMark = $state('');
	$effect(() => {
		const mark = settledMark(chat.view);
		if (mark) runMark = mark;
	});

	$effect(() => {
		const id = openId;
		if (!ready || !id) return;
		const mark = runMark;
		untrack(() => {
			const snap = data.snapshot;
			if (!snap) return;
			const have = forGoal(helpers, id, null);
			const library = have ? null : engine.cachedPersonas();
			const runs = have ? null : engine.cachedRuns({ goalId: id });
			if (library && runs) helpers = tagged(id, { library, runs });
			const read = coachesRead === id ? have : null;
			if (coachesFor === id && (!read || !mark || markRead(mark, read.runs))) return;
			coachesFor = id;
			const deps = read ? engine : { personas: () => engine.personas(), runs: () => runsOnOpen(id) };
			void loadCoaches(snap, id, deps, read?.library).then((r) => {
				if (page.params.id !== id) return;
				helpers = tagged(id, { library: r.views, runs: r.runs });
				coachesRead = id;
			});
		});
	});

	$effect(() => {
		const id = goal?.id;
		if (!s || !id) return;
		listPlugins(id).then(
			(list) => page.params.id === id && (pluginRows = tagged(id, list)),
			(e) => (data.error = String(e))
		);
	});

	const coaches = $derived<CoachView[]>(s && goal && mine ? coachesOn(mine.library, s.personas, mine.runs, goal.id) : []);

	const shown = $derived(activeVersion(plugins));
	const view = $derived.by(() => {
		if (!s || !goal) return null;
		if (!shown) return goalView(s, goal, clock);
		try {
			return resolve(parseTree(shown.tree), s, goal, clock);
		} catch {
			return goalView(s, goal, clock);
		}
	});
	const versions = $derived(
		plugins.length
			? [
					{ value: DERIVED, label: 'Derived' },
					...plugins.filter((p, i) => p.tree !== null && (i < SHOWN_VERSIONS || p.active)).map((p) => ({ value: p.id, label: `v${p.version}` }))
				]
			: []
	);
	const controls = $derived<AgentSettings | null>(goal ? { autonomy: goal.autonomy, reminders: goal.reminders, useCalendars: goal.use_calendars, runLimit: goal.run_limit_minutes } : null);
	let pace = $state<{ id: string; pace: Pace | null } | null>(null);
	$effect(() => {
		const id = goal?.id;
		if (!id || pace?.id === id) return;
		dashboard().then(
			(d) => page.params.id === id && (pace = { id, pace: d.goals.find((g) => g.goal_id === id)?.pace ?? null }),
			() => null
		);
	});
	const proposed = $derived(!!s?.proposals.some((p) => p.goal_id === goal?.id && p.status === 'proposed'));
	const chip = $derived(chipFor(goal ?? { status: 'active' }, chat.view ? runState(chat.view.status.status, chat.view.status.reason?.kind) : null, proposed, pace?.id === goal?.id ? (pace?.pace ?? null) : null));
	const status = $derived(goalStatus(chat.view, goal?.run_limit_minutes ?? settingsLimit, now, chip, chat.unreachable));
	const blocked = $derived(planWaits(chat.view));
	const planContext = $derived<PlanContext | undefined>(
		s && goal ? { busy: s.busy ?? [], actions: s.actions, goalId: goal.id, deadline: goal.deadline, knownUntil: shiftDay(formatYmd(clock.now, clock.zone), AHEAD_DAYS) } : undefined
	);

	/**
	 * Saves changed agent controls (the autonomy level, or reminders, calendars and the run
	 * limit), then syncs so the engine reads them on the next run or retry, and reloads.
	 */
	function oncontrols(v: AgentSettings) {
		const g = goal;
		if (!g) return;
		void data.run(async () => {
			if (v.autonomy !== g.autonomy) await client.setAutonomy(g.id, v.autonomy);
			else await client.setGoalControls(g.id, v.reminders, v.useCalendars, v.runLimit);
			await client.sync().catch(() => null);
			await data.load();
		});
	}

	/** Saves the goal's "Keep sessions in order" switch (Q79) and syncs it for the next replan. */
	function onordered(on: boolean) {
		const g = goal;
		if (!g) return;
		void data.run(async () => {
			await client.setGoalOrdered(g.id, on);
			await client.sync().catch(() => null);
			await data.load();
		});
	}

	/** Reads the engine again when it could not be reached; else syncs first, so a time limit just changed in the agents sheet reaches the engine, then retries. */
	async function onretry() {
		if (chat.unreachable) return chat.open();
		await client.sync().catch(() => null);
		await chat.retry();
	}

	let askRunAgain = $state(false);
	let rerun = $state(false);

	$effect(() => {
		if (!blocked) return;
		askRunAgain = false;
		rerun = false;
	});

	/** Syncs first, so the engine plans from the newest rows, then runs the goal again (Q74) with `answers`, or the saved ones. */
	async function runAgain(answers?: RunAnswer[]) {
		askRunAgain = false;
		rerun = false;
		await client.sync().catch(() => null);
		await chat.runAgain(answers);
	}

	let confirmDelete = $state(false);
	let deleting = $state(false);

	$effect.pre(() => {
		void page.params.id;
		untrack(() => {
			failed = [];
			retries.clear();
			runMark = '';
			askRunAgain = false;
			rerun = false;
			confirmDelete = false;
		});
	});

	/** Deletes the goal (Q68), keeping its actions under Routines when `keepActions`, then opens Goals. */
	async function deleteGoal(keepActions: boolean) {
		const id = goal?.id;
		if (!id || deleting) return;
		deleting = true;
		try {
			await client.deleteGoal(id, keepActions);
			confirmDelete = false;
			await goto('/goals');
			await data.load();
		} catch (e) {
			data.error = String(e);
		} finally {
			deleting = false;
		}
	}

	/** Switches the dashboard to a plugin version, or to the derived view. */
	function onversion(value: string) {
		const id = goal?.id;
		if (!id) return;
		void data.run(async () => {
			const list = await enablePlugin(id, value === DERIVED ? null : value);
			if (page.params.id === id) pluginRows = tagged(id, list);
		});
	}
</script>

{#if s && goal && view && routines && places}
	<RoutinesScreen goal={goalHeader(s, goal, clock.zone)} view={goalView(s, goal, clock)} routines={location.routineRows(places, clock.now, clock.zone)} />
{:else if s && goal && view && controls}
	{#key chat}
		<GoalCard
			goal={goalHeader(s, goal, clock.zone)}
			{view}
			{status}
			loading={!chat.known}
			steps={progressLines(chat.view?.events ?? [])}
			research={chat.origin?.research ?? null}
			answers={chat.origin?.answers ?? []}
			messages={chat.thread}
			onchat={(e) => chat.handle(e)}
			voice={speech}
			{controls}
			defaultLimit={settingsLimit}
			{oncontrols}
			{coaches}
			helpersLoading={helpersFor !== goal.id}
			plugins={pluginsUsed(plugins, goalRuns)}
			pluginHref={(name) => pluginPageFor(plugins, name, goal.id)}
			oncoaches={() => void goto(`/personas?goal=${goal.id}`)}
			{versions}
			version={shown?.id ?? DERIVED}
			{onversion}
			onstop={() => void chat.stop()}
			onretry={() => void onretry()}
			onanswers={(changed) => chat.handle({ type: 'send', text: answersReason(changed) })}
			{rerun}
			onrerun={(answers) => void runAgain(answers)}
			oncancelrerun={() => (rerun = false)}
			files={[...goalAttachments(s.files ?? [], goal.id), ...failed]}
			{upload}
			onfiles={(list) => void uploadFiles(list)}
			onretryfile={(key) => void retryUpload(key)}
			onlink={(url) => attaching(() => client.attachLink(goal.id, url))}
			{planContext}
			ordered={goal.ordered}
			{onordered}
			onremove={(fid) => (retries.has(fid) ? ((retries.delete(fid)), (failed = failed.filter((f) => f.id !== fid))) : attaching(() => client.removeGoalFile(fid)))}
		/>
	{/key}
	{#if places && goal.id !== places.routines_goal}
		<div class="flex flex-col gap-2 px-4 pt-2 pb-6">
			{#if !status.stop}
				<button type="button" class="min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:text-muted disabled:opacity-60" disabled={blocked || !chat.known} aria-describedby={blocked ? 'run-again-blocked' : undefined} onclick={() => (askRunAgain = true)}>Run again</button>
				{#if blocked}<p id="run-again-blocked" class="text-sm text-muted">Approve or decline the waiting plan first.</p>{/if}
			{/if}
			<button type="button" class="min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (confirmDelete = true)}>Delete goal</button>
		</div>
	{/if}
	{#if askRunAgain && !blocked}
		<RunAgainSheet title={goal.title} answers={(chat.origin?.answers ?? []).length > 0} onkeep={() => void runAgain()} onedit={() => ((askRunAgain = false), (rerun = true))} onclose={() => (askRunAgain = false)} />
	{/if}
	{#if confirmDelete}
		<DeleteGoalSheet title={goal.title} busy={deleting} ondelete={(keep) => void deleteGoal(keep)} onclose={() => (confirmDelete = false)} />
	{/if}
{:else if s}
	<div class="flex flex-col gap-3 p-4">
		<p class="text-sm text-muted">This goal was deleted.</p>
		<button type="button" class="min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => void goto('/goals')}>Back to Goals</button>
	</div>
{/if}
