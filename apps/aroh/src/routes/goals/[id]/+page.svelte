<script lang="ts">
	import { page } from '$app/state';
	import '../../../lib/calendar/calendar.css';
	import ProgressBar from '../../../lib/plugins/ProgressBar.svelte';
	import PluginSlot from '../../../lib/plugins/PluginSlot.svelte';
	import { byDay, errorText, formatMinutes, goalDetail, type Action, type GoalDetail } from '../../../lib/goals/api';
	import { formatDayTitle, formatRange } from '../../../lib/calendar/time';

	const STATUS_TEXT: Record<Action['status'], string> = {
		planned: 'Planned',
		done: 'Done',
		missed: 'Missed',
		skipped: 'Skipped'
	};

	const id = $derived(page.params.id ?? '');

	let detail = $state<GoalDetail | null>(null);
	let error = $state('');

	const done = $derived(detail ? detail.actions.filter((a) => a.status === 'done').length : 0);
	const total = $derived(detail ? detail.actions.length : 0);
	const doneMinutes = $derived(
		detail ? detail.actions.filter((a) => a.status === 'done').reduce((s, a) => s + a.est_minutes, 0) : 0
	);
	const totalMinutes = $derived(detail ? detail.actions.reduce((s, a) => s + a.est_minutes, 0) : 0);
	const days = $derived(detail ? byDay(detail.actions) : []);
	const next = $derived(detail?.actions.find((a) => a.status === 'planned' && new Date(a.end_at) > new Date()) ?? null);

	async function load(goalId: string) {
		error = '';
		try {
			detail = await goalDetail(goalId);
		} catch (err) {
			error = errorText(err);
		}
	}

	$effect(() => {
		if (id) load(id);
	});
</script>

{#if error}
	<div class="error" role="alert">
		<p>Could not load this goal. {error}</p>
		<button type="button" class="quiet" onclick={() => load(id)}>Retry</button>
	</div>
{:else if !detail}
	<p class="muted" aria-busy="true">Loading goal</p>
{:else}
	<div class="detail">
		<header>
			<h1>{detail.goal.title}</h1>
			<p class="sub num">
				Due {detail.goal.target_date}
				{#if detail.goal.source_url}· <a href={detail.goal.source_url} target="_blank" rel="noopener noreferrer">Course page</a>{/if}
			</p>
		</header>

		{#if detail.goal.status === 'draft'}
			<section class="panel">
				<h2>No approved plan yet</h2>
				<p class="muted">
					{detail.proposed.length > 0
						? `A plan of ${detail.proposed.length} sessions is waiting for approval.`
						: 'Finish the questions to get a plan.'}
				</p>
				<a class="quiet link" href="/goals/new">Start a new goal</a>
			</section>
		{:else}
			<div class="progress">
				<ProgressBar props={{ label: 'Sessions done', value: done, max: total }} data={{}} />
			</div>

			<section class="panel stats" aria-label="Plan summary">
				<div><span class="muted">Studied</span><strong class="num">{formatMinutes(doneMinutes)} of {formatMinutes(totalMinutes)}</strong></div>
				<div><span class="muted">Next</span><strong class="num">{next ? `${formatDayTitle(new Date(next.start_at))}, ${formatRange(next.start_at, next.end_at)}` : 'Nothing planned'}</strong></div>
				{#if detail.plan}
					<div><span class="muted">Plan</span><span>Version {detail.plan.version}{detail.plan.summary ? `. ${detail.plan.summary}` : ''}</span></div>
				{/if}
			</section>
		{/if}

		<PluginSlot name="goal.detail" goalId={id} />

		{#if days.length > 0}
			<section aria-labelledby="sessions">
				<h2 id="sessions">Sessions</h2>
				<ol class="days">
					{#each days as group (group.day.toISOString())}
						<li>
							<h3><span>{formatDayTitle(group.day)}</span><span class="num muted">{formatMinutes(group.minutes)}</span></h3>
							<ul>
								{#each group.actions as action (action.id)}
									<li class="session" class:finished={action.status !== 'planned'}>
										<span class="time num">{formatRange(action.start_at, action.end_at)}</span>
										<span class="what">{action.title}</span>
										<span class="state state-{action.status}">{STATUS_TEXT[action.status]}</span>
									</li>
								{/each}
							</ul>
						</li>
					{/each}
				</ol>
			</section>
		{/if}

		{#if detail.analysis}
			<section aria-labelledby="modules">
				<h2 id="modules">Course modules</h2>
				<ul class="modules panel">
					{#each detail.analysis.modules as module, i (i)}
						<li><span>{module.title}</span><span class="num muted">{formatMinutes(module.minutes)}</span></li>
					{/each}
				</ul>
			</section>
		{/if}
	</div>
{/if}

<style>
	.detail {
		max-width: 560px;
		margin: 0 auto;
		display: grid;
		gap: var(--space-3);
		padding-bottom: var(--space-5);
	}
	header h1 {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.sub {
		margin: var(--space-1) 0 0;
		color: var(--cal-muted);
	}
	.sub a {
		color: var(--cal-accent);
	}
	h2 {
		margin: 0 0 var(--space-2);
		font-size: 1.0625rem;
		font-weight: 600;
	}
	.panel {
		margin: 0;
		padding: var(--space-3);
		background: var(--cal-raised);
		border: 1px solid var(--cal-line);
		border-radius: 12px;
	}
	.panel h2 {
		margin-bottom: var(--space-1);
	}
	.muted {
		color: var(--cal-muted);
		font-weight: 400;
	}
	.panel p {
		margin: 0 0 var(--space-2);
	}
	.progress {
		--aroh-track: color-mix(in srgb, var(--cal-line) 70%, var(--cal-surface));
		--aroh-accent: var(--cal-accent);
		--aroh-muted: var(--cal-muted);
	}
	.progress :global(.card) {
		padding: var(--space-3);
		background: var(--cal-raised);
		border: 1px solid var(--cal-line);
		border-radius: 12px;
	}
	.stats {
		display: grid;
		gap: 12px;
	}
	.stats div {
		display: grid;
		gap: 2px;
	}
	.stats span.muted {
		font-size: 0.8125rem;
	}
	.stats strong {
		font-weight: 600;
	}
	.quiet {
		min-height: var(--tap);
		padding: 0 var(--space-3);
		border: 1px solid var(--cal-line);
		border-radius: 12px;
		background: var(--cal-raised);
		color: var(--cal-ink);
		font: inherit;
		font-weight: 600;
		cursor: pointer;
	}
	.link {
		display: inline-flex;
		align-items: center;
		text-decoration: none;
	}
	.error {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 12px var(--space-3);
		border-radius: 12px;
		background: color-mix(in srgb, var(--cal-danger) 10%, var(--cal-raised));
	}
	.error p {
		margin: 0;
	}
	.days,
	.days ul,
	.modules {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 6px;
	}
	.days {
		gap: var(--space-3);
	}
	.days h3 {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
		margin: 0 0 var(--space-2);
		font-size: 0.9375rem;
		font-weight: 600;
	}
	.session {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		column-gap: 12px;
		align-items: baseline;
		min-height: var(--tap);
		padding: 10px 12px;
		background: var(--cal-raised);
		border: 1px solid var(--cal-line);
		border-left: 4px solid var(--goal-0);
		border-radius: 8px;
	}
	.session.finished .what {
		color: var(--cal-muted);
	}
	.time,
	.state {
		font-size: 0.8125rem;
		color: var(--cal-muted);
		white-space: nowrap;
	}
	.state-done {
		color: var(--cal-accent);
		font-weight: 600;
	}
	.state-missed {
		color: var(--cal-danger);
		font-weight: 600;
	}
	.what {
		font-weight: 500;
		overflow-wrap: anywhere;
	}
	.modules {
		padding: 4px var(--space-3);
		gap: 0;
	}
	.modules li {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 10px 0;
		border-bottom: 1px solid var(--cal-line);
	}
	.modules li:last-child {
		border-bottom: 0;
	}
	:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 2px;
	}
	@media (max-width: 380px) {
		.session {
			grid-template-columns: minmax(0, 1fr) auto;
		}
		.time {
			grid-column: 1 / -1;
		}
	}
</style>
