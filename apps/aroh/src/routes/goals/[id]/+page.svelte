<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import GoalCard from '$lib/ui/components/goal/GoalCard.svelte';
	import RoutinesScreen from '$lib/ui/screens/RoutinesScreen.svelte';
	import * as location from '$lib/app/location';
	import { onMount } from 'svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { goalHeader, goalView } from '$lib/app/views';
	import { engine, progressLines } from '$lib/app/engine';
	import { GoalChat } from '$lib/app/chat.svelte';
	import { answersReason, goalStatus } from '$lib/app/goalcard';
	import type { AgentSettings } from '$lib/app/newgoal';
	import { speech } from '$lib/app/speech.svelte';
	import { activeVersion, enablePlugin, listPlugins, type PluginVersion } from '$lib/plugins/api';
	import { parseTree, resolve } from '$lib/plugins/resolve';
	import type { CoachView } from '$lib/ui/screens/types';
	import { loadCoaches } from '$lib/app/personas';
	import { goalAttachments, pendingAttachment, uploadFailed } from '$lib/app/files';
	import type { Attachment } from '$lib/ui/components/goal/Attachments.svelte';
	import DeleteGoalSheet from '$lib/ui/components/goal/DeleteGoalSheet.svelte';
	import RunAgainSheet from '$lib/ui/components/goal/RunAgainSheet.svelte';
	import type { RunAnswer } from '$lib/app/engine';
	import type { PlanContext } from '$lib/ui/components/plan/PlanCard.svelte';
	import { AHEAD_DAYS } from '$lib/app/calendar';
	import { formatYmd, shiftDay } from '$lib/ui/format';

	const DERIVED = 'derived';
	const SHOWN_VERSIONS = 3;
	const TICK_MS = 1000;
	const DEFAULT_LIMIT = 5;

	const clock = getClock();
	const s = $derived(data.snapshot);
	const goal = $derived(s?.goals.find((g) => g.id === page.params.id));
	let plugins = $state<PluginVersion[]>([]);
	let places = $state<location.LocationView | null>(null);
	const routines = $derived(!!goal && !!places && goal.id === places.routines_goal);
	let coaches = $state<CoachView[]>([]);
	let settingsLimit = $state(DEFAULT_LIMIT);
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

	const chat = $derived(new GoalChat(page.params.id ?? null, 'Tell the agents anything about this goal. They read it at their next step.'));
	$effect(() => void chat.open());

	$effect(() => {
		const id = goal?.id;
		if (s && id) void loadCoaches(s, id).then((r) => (coaches = r.coaches));
	});

	$effect(() => {
		const id = goal?.id;
		if (!s || !id) return;
		listPlugins(id).then(
			(list) => (plugins = list),
			(e) => (data.error = String(e))
		);
	});

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
	const status = $derived(goalStatus(chat.view, goal?.run_limit_minutes ?? settingsLimit, now));
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

	/** Syncs first, so a time limit just changed in the agents sheet reaches the engine, then retries. */
	async function onretry() {
		await client.sync().catch(() => null);
		await chat.retry();
	}

	let askRunAgain = $state(false);
	let rerun = $state(false);

	/** Syncs first, so the engine plans from the newest rows, then runs the goal again (Q74) with `answers`, or the saved ones. */
	async function runAgain(answers?: RunAnswer[]) {
		askRunAgain = false;
		rerun = false;
		await client.sync().catch(() => null);
		await chat.runAgain(answers);
	}

	let confirmDelete = $state(false);
	let deleting = $state(false);

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
		if (goal) void data.run(async () => (plugins = await enablePlugin(goal.id, value === DERIVED ? null : value)));
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
			oncoaches={() => void goto(`/personas?goal=${goal.id}`)}
			{versions}
			version={shown?.id ?? DERIVED}
			{onversion}
			onstop={() => void chat.stop()}
			onretry={() => void onretry()}
			onanswers={(changed) => chat.handle({ type: 'send', text: answersReason(changed) })}
			{rerun}
			onrerun={(answers) => void runAgain(answers)}
			files={[...goalAttachments(s.files ?? [], goal.id), ...failed]}
			{upload}
			onfiles={(list) => void uploadFiles(list)}
			onretryfile={(key) => void retryUpload(key)}
			onlink={(url) => attaching(() => client.attachLink(goal.id, url))}
			{planContext}
			onremove={(fid) => (retries.has(fid) ? ((retries.delete(fid)), (failed = failed.filter((f) => f.id !== fid))) : attaching(() => client.removeGoalFile(fid)))}
		/>
	{/key}
	{#if places && goal.id !== places.routines_goal}
		<div class="flex flex-col gap-2 px-4 pt-2 pb-6">
			{#if !status.stop}<button type="button" class="min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (askRunAgain = true)}>Run again</button>{/if}
			<button type="button" class="min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (confirmDelete = true)}>Delete goal</button>
		</div>
	{/if}
	{#if askRunAgain}
		<RunAgainSheet title={goal.title} answers={(chat.origin?.answers ?? []).length > 0} onkeep={() => void runAgain()} onedit={() => ((askRunAgain = false), (rerun = true))} onclose={() => (askRunAgain = false)} />
	{/if}
	{#if confirmDelete}
		<DeleteGoalSheet title={goal.title} busy={deleting} ondelete={(keep) => void deleteGoal(keep)} onclose={() => (confirmDelete = false)} />
	{/if}
{:else if s}
	<p class="p-4 text-sm text-muted">This goal no longer exists.</p>
{/if}
