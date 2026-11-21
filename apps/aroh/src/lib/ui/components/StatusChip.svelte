<script lang="ts" module>
	/** Status tones, each mapped to one --status-* token. Action statuses use the same names. */
	export type Tone = 'done' | 'planned' | 'warn' | 'missed' | 'skipped';

	/** Background class of a status dot per tone. */
	export const DOT: Record<Tone, string> = {
		done: 'bg-status-done',
		planned: 'bg-status-planned',
		warn: 'bg-status-warn',
		missed: 'bg-status-missed',
		skipped: 'bg-status-skipped'
	};

	const CHIP: Record<Tone, string> = {
		done: 'border-status-done/40 bg-status-done/10 text-status-done',
		planned: 'border-status-planned/40 bg-status-planned/10 text-status-planned',
		warn: 'border-status-warn/40 bg-status-warn/10 text-status-warn',
		missed: 'border-status-missed/40 bg-status-missed/10 text-status-missed',
		skipped: 'border-border bg-surface-2 text-status-skipped'
	};
</script>

<script lang="ts">
	/** Small status chip: a tinted hairline box with the status word in its tone. */
	let { tone, label }: { tone: Tone; label: string } = $props();
</script>

<span class="inline-flex shrink-0 items-center rounded-sm border px-1.5 text-xs leading-5 font-medium {CHIP[tone]}">{label}</span>
