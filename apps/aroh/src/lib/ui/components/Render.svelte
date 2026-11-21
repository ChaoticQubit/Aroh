<script lang="ts">
	import type { Component } from 'svelte';
	import Capture from './inputs/Capture.svelte';
	import CheckOff from './inputs/CheckOff.svelte';
	import Choice from './inputs/Choice.svelte';
	import Counter from './inputs/Counter.svelte';
	import Note from './inputs/Note.svelte';
	import NumberInput from './inputs/NumberInput.svelte';
	import Rating from './inputs/Rating.svelte';
	import Timer from './inputs/Timer.svelte';
	import BarChart from './display/BarChart.svelte';
	import LineChart from './display/LineChart.svelte';
	import LogList from './display/LogList.svelte';
	import PhotoTimeline from './display/PhotoTimeline.svelte';
	import ProgressRing from './display/ProgressRing.svelte';
	import StatTile from './display/StatTile.svelte';
	import StreakHeatmap from './display/StreakHeatmap.svelte';
	import TargetVsActual from './display/TargetVsActual.svelte';
	import ActionList from './planning/ActionList.svelte';
	import Countdown from './planning/Countdown.svelte';
	import MilestoneTimeline from './planning/MilestoneTimeline.svelte';
	import ModuleChecklist from './planning/ModuleChecklist.svelte';
	import WeekStrip from './planning/WeekStrip.svelte';
	import InsightCard from './agent/InsightCard.svelte';
	import ProposalCard from './agent/ProposalCard.svelte';
	import QuestionCard from './agent/QuestionCard.svelte';
	import type { ComponentSpec, OnChange } from '$lib/ui/contract';

	/** Renders any component spec with its component. The only place that maps kinds to components. */
	let { spec, onchange }: { spec: ComponentSpec; onchange?: OnChange } = $props();

	/** Every kind mapped to its component; a missing kind fails type checking. */
	const KINDS: Record<ComponentSpec['kind'], Component<{ spec: never; onchange?: OnChange }>> = {
		number_input: NumberInput,
		rating: Rating,
		choice: Choice,
		note: Note,
		check_off: CheckOff,
		counter: Counter,
		timer: Timer,
		capture: Capture,
		line_chart: LineChart,
		bar_chart: BarChart,
		progress_ring: ProgressRing,
		target_vs_actual: TargetVsActual,
		streak_heatmap: StreakHeatmap,
		stat_tile: StatTile,
		log_list: LogList,
		photo_timeline: PhotoTimeline,
		action_list: ActionList,
		week_strip: WeekStrip,
		milestone_timeline: MilestoneTimeline,
		module_checklist: ModuleChecklist,
		countdown: Countdown,
		insight_card: InsightCard,
		proposal_card: ProposalCard,
		question_card: QuestionCard
	};

	const Kind = $derived(KINDS[spec.kind] as Component<{ spec: ComponentSpec; onchange?: OnChange }>);
</script>

<Kind {spec} {onchange} />
