<script lang="ts">
	import { goto } from '$app/navigation';
	import '../../../lib/calendar/calendar.css';
	import {
		analyzeGoal,
		approvePlan,
		byDay,
		createGoal,
		errorText,
		formatMinutes,
		isMissingKey,
		planGoal,
		type Analysis,
		type Goal,
		type PlanView
	} from '../../../lib/goals/api';
	import { formatDayTitle, formatRange } from '../../../lib/calendar/time';

	type Step = 'course' | 'reading' | 'questions' | 'planning' | 'plan';

	const MAX_UPLOAD_BYTES = 1_000_000;

	function localDate(d: Date): string {
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
	}

	const today = localDate(new Date());

	let step = $state<Step>('course');
	let source = $state<'link' | 'paste'>('link');
	let title = $state('');
	let link = $state('');
	let pasted = $state('');
	let fileName = $state('');
	let targetDate = $state(localDate(new Date(Date.now() + 30 * 86_400_000)));
	let error = $state('');
	let missingKey = $state(false);
	let busy = $state(false);

	let goal = $state<Goal | null>(null);
	let analysis = $state<Analysis | null>(null);
	let answers = $state<Record<string, string>>({});
	let proposal = $state<PlanView | null>(null);

	const days = $derived(proposal ? byDay(proposal.actions) : []);
	const plannedMinutes = $derived(proposal ? proposal.actions.reduce((sum, a) => sum + a.est_minutes, 0) : 0);
	const answered = $derived(analysis ? analysis.questions.filter((q) => (answers[q.id] ?? '').trim() !== '').length : 0);
	const canStart = $derived(
		title.trim() !== '' && targetDate >= today && (source === 'link' ? link.trim() !== '' : pasted.trim() !== '')
	);

	function fail(err: unknown, back: Step) {
		missingKey = isMissingKey(err);
		error = errorText(err);
		step = back;
	}

	async function readFile(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		if (file.size > MAX_UPLOAD_BYTES) {
			error = 'That file is over 1 MB. Paste the part you need instead.';
			input.value = '';
			return;
		}
		pasted = await file.text();
		fileName = file.name;
		if (title.trim() === '') title = file.name.replace(/\.(txt|md)$/i, '');
	}

	async function start(event: SubmitEvent) {
		event.preventDefault();
		if (!canStart || busy) return;
		busy = true;
		error = '';
		missingKey = false;
		step = 'reading';
		try {
			const url = source === 'link' ? link.trim() : null;
			if (!goal || goal.title !== title.trim() || goal.source_url !== url || goal.target_date !== targetDate) {
				goal = await createGoal(title, url, targetDate);
			}
			analysis = await analyzeGoal(goal.id, source === 'paste' ? pasted : null);
			answers = Object.fromEntries(analysis.questions.map((q) => [q.id, answers[q.id] ?? '']));
			step = 'questions';
		} catch (err) {
			fail(err, 'course');
		} finally {
			busy = false;
		}
	}

	async function makePlan(event?: SubmitEvent) {
		event?.preventDefault();
		if (!goal || !analysis || busy) return;
		busy = true;
		error = '';
		missingKey = false;
		step = 'planning';
		const byText: Record<string, string> = {};
		for (const q of analysis.questions) {
			const answer = (answers[q.id] ?? '').trim();
			if (answer !== '') byText[q.text] = answer;
		}
		try {
			proposal = await planGoal(goal.id, byText);
			step = 'plan';
		} catch (err) {
			fail(err, 'questions');
		} finally {
			busy = false;
		}
	}

	async function approve() {
		if (!goal || !proposal || busy) return;
		busy = true;
		error = '';
		try {
			await approvePlan(proposal.plan.id);
			await goto(`/goals/${goal.id}`);
		} catch (err) {
			error = errorText(err);
		} finally {
			busy = false;
		}
	}
</script>

<div class="wizard">
	<header>
		<h1>New goal</h1>
		<p class="sub">
			{#if step === 'course'}Point Aroh at a course and the day you want it finished.
			{:else if step === 'reading'}Reading {source === 'link' ? 'the course page' : 'your course text'}.
			{:else if step === 'questions'}{analysis?.questions.length} questions so the plan fits your days.
			{:else if step === 'planning'}Fitting {analysis ? formatMinutes(analysis.analysis.total_minutes) : ''} of study around your calendar.
			{:else}Check the plan, then approve it to put it on your calendar.{/if}
		</p>
	</header>

	{#if error}
		<div class="error" role="alert">
			<p>{error}</p>
			{#if missingKey}<a href="/settings">Open Settings</a>{/if}
		</div>
	{/if}

	{#if step === 'course'}
		<form class="panel" onsubmit={start}>
			<label for="title">What do you want to finish?</label>
			<input id="title" bind:value={title} maxlength="200" placeholder="Rust for beginners" autocomplete="off" />

			<span class="label" id="source-label">Course source</span>
			<div class="segmented" role="radiogroup" aria-labelledby="source-label">
				<button type="button" role="radio" aria-checked={source === 'link'} onclick={() => (source = 'link')}>Link</button>
				<button type="button" role="radio" aria-checked={source === 'paste'} onclick={() => (source = 'paste')}>Paste or file</button>
			</div>

			{#if source === 'link'}
				<label for="link">Public course page</label>
				<input
					id="link"
					type="url"
					inputmode="url"
					autocapitalize="off"
					spellcheck="false"
					bind:value={link}
					placeholder="https://"
					maxlength="2048"
				/>
				<p class="hint">Aroh reads public pages only. For a course behind a login, paste its outline instead.</p>
			{:else}
				<label for="pasted">Course outline or syllabus</label>
				<textarea id="pasted" bind:value={pasted} rows="7" placeholder="Module 1: Getting started (45 min)"></textarea>
				<label class="file">
					<input type="file" accept=".txt,.md,text/plain,text/markdown" onchange={readFile} />
					<span>{fileName ? `Loaded ${fileName}` : 'Choose a .txt or .md file'}</span>
				</label>
			{/if}

			<label for="target">Finish by</label>
			<input id="target" type="date" min={today} bind:value={targetDate} />

			<button class="primary" type="submit" disabled={!canStart || busy}>Read the course</button>
		</form>
	{:else if step === 'reading' || step === 'planning'}
		<section class="panel progress" aria-live="polite" aria-busy="true">
			<ol class="stages">
				<li class:done={step === 'planning'} class:active={step === 'reading'}>
					{source === 'link' ? 'Read the course page' : 'Read your course text'}
				</li>
				<li class:done={step === 'planning'} class:active={step === 'reading'}>Measure each module</li>
				<li class:done={step === 'planning'} class:active={step === 'reading'}>Write your questions</li>
				<li class:active={step === 'planning'}>Plan every session up to {targetDate}</li>
			</ol>
			<p class="hint">This takes up to a minute. Keep the app open.</p>
		</section>
	{:else if step === 'questions' && analysis}
		<section class="panel summary" aria-labelledby="course-title">
			<h2 id="course-title">{analysis.analysis.title}</h2>
			<p class="num">{analysis.analysis.modules.length} modules, {formatMinutes(analysis.analysis.total_minutes)} in total</p>
		</section>
		<form class="questions" onsubmit={makePlan}>
			{#each analysis.questions as q (q.id)}
				<fieldset class="panel">
					<legend>{q.text}</legend>
					{#if q.options.length > 0}
						<div class="chips">
							{#each q.options as option (option)}
								<button
									type="button"
									class="chip"
									aria-pressed={answers[q.id] === option}
									onclick={() => (answers[q.id] = answers[q.id] === option ? '' : option)}>{option}</button
								>
							{/each}
						</div>
					{:else}
						<input aria-label={q.text} bind:value={answers[q.id]} maxlength="500" />
					{/if}
				</fieldset>
			{/each}
			<div class="footer">
				<span class="num">{answered} of {analysis.questions.length} answered</span>
				<button class="primary" type="submit" disabled={busy}>Make my plan</button>
			</div>
		</form>
	{:else if step === 'plan' && proposal}
		<section class="panel summary" aria-labelledby="plan-title">
			<h2 id="plan-title">{proposal.actions.length} sessions, {formatMinutes(plannedMinutes)}</h2>
			<p class="num">{days.length} study days, last one {formatDayTitle(days[days.length - 1].day)}. Due {targetDate}.</p>
			{#if proposal.plan.summary}<p>{proposal.plan.summary}</p>{/if}
		</section>
		<ol class="days">
			{#each days as group (group.day.toISOString())}
				<li>
					<h3><span>{formatDayTitle(group.day)}</span><span class="num muted">{formatMinutes(group.minutes)}</span></h3>
					<ul>
						{#each group.actions as action (action.id)}
							<li class="session">
								<span class="time num">{formatRange(action.start_at, action.end_at)}</span>
								<span class="what">{action.title}</span>
								{#if action.notes}<span class="notes">{action.notes}</span>{/if}
							</li>
						{/each}
					</ul>
				</li>
			{/each}
		</ol>
		<div class="sticky">
			<button type="button" class="quiet" onclick={() => (step = 'questions')} disabled={busy}>Change answers</button>
			<button type="button" class="primary" onclick={approve} disabled={busy}>Approve plan</button>
		</div>
	{/if}
</div>

<style>
	.wizard {
		max-width: 560px;
		margin: 0 auto;
		display: grid;
		gap: var(--space-3);
		padding-bottom: var(--space-5);
	}
	header h1 {
		margin: 0;
	}
	.sub {
		margin: var(--space-1) 0 0;
		color: var(--cal-muted);
	}
	.panel {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: var(--space-3);
		background: var(--cal-raised);
		border: 1px solid var(--cal-line);
		border-radius: 12px;
		min-width: 0;
	}
	label,
	.label,
	legend {
		font-size: 0.875rem;
		font-weight: 500;
		margin-top: var(--space-1);
	}
	legend {
		float: left;
		width: 100%;
		margin: 0 0 var(--space-2);
		padding: 0;
		font-size: 1rem;
		line-height: 1.35;
	}
	input,
	textarea {
		width: 100%;
		min-height: var(--tap);
		padding: 10px 12px;
		border: 1px solid var(--cal-line);
		border-radius: 10px;
		background: var(--cal-surface);
		font: inherit;
		font-size: 1rem;
		color: var(--cal-ink);
	}
	textarea {
		resize: vertical;
		line-height: 1.45;
	}
	.hint {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--cal-muted);
	}
	.segmented {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 4px;
		padding: 4px;
		background: var(--cal-surface);
		border-radius: 12px;
	}
	.segmented button {
		min-height: 40px;
		border: 0;
		border-radius: 9px;
		background: transparent;
		color: var(--cal-muted);
		font-weight: 500;
	}
	.segmented button[aria-checked='true'] {
		background: var(--cal-raised);
		color: var(--cal-ink);
		box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
	}
	.file {
		position: relative;
		display: flex;
		align-items: center;
		min-height: var(--tap);
		padding: 0 12px;
		border: 1px dashed var(--cal-line);
		border-radius: 10px;
		color: var(--cal-accent);
		font-weight: 500;
	}
	.file input {
		position: absolute;
		inset: 0;
		opacity: 0;
		min-height: 0;
	}
	button {
		font: inherit;
		cursor: pointer;
	}
	.primary,
	.quiet {
		min-height: var(--tap);
		padding: 0 var(--space-3);
		border-radius: 12px;
		font-weight: 600;
		transition: transform var(--ease), opacity var(--ease);
	}
	.primary {
		border: 0;
		background: var(--cal-accent);
		color: var(--cal-accent-ink);
	}
	form.panel .primary {
		margin-top: var(--space-2);
	}
	.quiet {
		border: 1px solid var(--cal-line);
		background: var(--cal-raised);
		color: var(--cal-ink);
	}
	.primary:active,
	.quiet:active,
	.chip:active {
		transform: scale(0.98);
	}
	button:disabled {
		opacity: 0.5;
		cursor: default;
	}
	:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 2px;
	}
	.error {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 12px var(--space-3);
		border-radius: 12px;
		background: color-mix(in srgb, var(--cal-danger) 10%, var(--cal-raised));
		color: var(--cal-ink);
	}
	.error p {
		margin: 0;
	}
	.error a {
		flex: none;
		min-height: var(--tap);
		display: inline-flex;
		align-items: center;
		color: var(--cal-danger);
		font-weight: 600;
	}
	.stages {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	.stages li {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: var(--tap);
		color: var(--cal-muted);
	}
	.stages li::before {
		content: '';
		flex: none;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 2px solid var(--cal-line);
	}
	.stages li.active {
		color: var(--cal-ink);
		font-weight: 500;
	}
	.stages li.active::before {
		border-color: var(--cal-accent);
		animation: pulse 1.2s ease-in-out infinite;
	}
	.stages li.done::before {
		border-color: var(--cal-accent);
		background: var(--cal-accent);
	}
	@keyframes pulse {
		50% {
			transform: scale(0.7);
		}
	}
	.summary h2 {
		margin: 0;
		font-size: 1.125rem;
		font-weight: 600;
	}
	.summary p {
		margin: 0;
		color: var(--cal-muted);
	}
	.questions {
		display: grid;
		gap: var(--space-2);
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.chip {
		min-height: var(--tap);
		padding: 0 14px;
		border: 1px solid var(--cal-line);
		border-radius: 999px;
		background: var(--cal-surface);
		color: var(--cal-ink);
		transition: transform var(--ease), background var(--ease);
	}
	.chip[aria-pressed='true'] {
		border-color: var(--cal-accent);
		background: color-mix(in srgb, var(--cal-accent) var(--tint), var(--cal-raised));
		font-weight: 600;
	}
	.footer,
	.sticky {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
	}
	.footer span {
		color: var(--cal-muted);
		font-size: 0.875rem;
	}
	.sticky {
		position: sticky;
		bottom: 0;
		padding: var(--space-2) 0 max(var(--space-2), env(safe-area-inset-bottom));
		background: var(--cal-surface);
	}
	.sticky .primary {
		flex: 1;
	}
	.days {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
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
	.muted {
		color: var(--cal-muted);
		font-weight: 400;
	}
	.days ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 6px;
	}
	.session {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		column-gap: 12px;
		align-items: baseline;
		min-height: var(--tap);
		padding: 10px 12px;
		background: var(--cal-raised);
		border: 1px solid var(--cal-line);
		border-left: 4px solid var(--goal-0);
		border-radius: 8px;
	}
	.time {
		font-size: 0.8125rem;
		color: var(--cal-muted);
		white-space: nowrap;
	}
	.what {
		font-weight: 500;
		overflow-wrap: anywhere;
	}
	.notes {
		grid-column: 2;
		font-size: 0.8125rem;
		color: var(--cal-muted);
	}
	@media (max-width: 380px) {
		.session {
			grid-template-columns: minmax(0, 1fr);
		}
		.notes {
			grid-column: 1;
		}
	}
</style>
