<script lang="ts">
	/**
	 * Sketch of the iOS lock screen with one Aroh nudge, for mockups only; the real banner is drawn by iOS.
	 * `expanded` shows the long-press state with the notification actions.
	 * `time` and `date` are the lock screen clock; `detail` names the due or missed action.
	 */
	let {
		time,
		date,
		title,
		body,
		detail,
		actions,
		expanded = false
	}: { time: string; date: string; title: string; body: string; detail: string; actions: string[]; expanded?: boolean } = $props();
</script>

<div class="flex h-full flex-col items-center gap-6 bg-surface-2 px-3 pt-20 text-fg {expanded ? 'justify-center pt-0' : ''}">
	{#if !expanded}
		<div class="flex flex-col items-center gap-1">
			<p class="text-sm font-medium text-muted">{date}</p>
			<p class="text-[64px] leading-none font-semibold tabular-nums">{time}</p>
		</div>
	{/if}
	<div class="flex w-full flex-col gap-2 {expanded ? '' : 'mt-auto mb-24'}">
		<div class="flex w-full flex-col gap-0.5 rounded-md border border-border bg-bg px-3 py-2.5">
			<p class="flex items-center justify-between text-xs text-muted">
				<span class="flex items-center gap-1.5"><span class="flex size-4 items-center justify-center rounded-sm bg-accent text-[9px] font-semibold text-accent-fg">A</span>AROH</span>
				<span>now</span>
			</p>
			<p class="text-sm font-semibold">{title}</p>
			<p class="text-sm">{body}</p>
			<p class="text-xs text-muted">{detail}</p>
		</div>
		{#if expanded}
			<ul class="flex flex-col divide-y divide-border overflow-hidden rounded-md border border-border bg-bg">
				{#each actions as a (a)}<li class="flex min-h-11 items-center px-3 text-sm">{a}</li>{/each}
			</ul>
		{/if}
	</div>
</div>
