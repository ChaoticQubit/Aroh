<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import type { SetupStep } from '$lib/app/setup';

	/**
	 * First-launch setup: one permission per screen with its one-line reason, an allow button and
	 * a skip. `onallow` asks the OS for that step; `ondone` runs after the last step, allowed or not.
	 */
	let { steps, onallow, ondone }: { steps: readonly SetupStep[]; onallow: (id: SetupStep['id']) => Promise<void>; ondone: () => void } = $props();

	let at = $state(0);
	let asking = $state(false);
	const step = $derived(steps[at]);

	/** Moves to the next step, or finishes after the last one. */
	function next() {
		if (at + 1 < steps.length) at += 1;
		else ondone();
	}

	/** Asks for the current step's access, then moves on whatever the owner chose. */
	async function allow() {
		asking = true;
		await onallow(step.id).catch(() => undefined);
		asking = false;
		next();
	}
</script>

<div class="flex h-full flex-col bg-bg px-6 pt-[calc(env(safe-area-inset-top)+3rem)] pb-[calc(env(safe-area-inset-bottom)+1.5rem)] text-fg">
	<p class="text-sm text-muted" aria-live="polite">Step {at + 1} of {steps.length}</p>
	<h1 class="mt-2 text-2xl font-semibold">{step.title}</h1>
	<p class="mt-3 text-base text-muted">{step.reason}</p>
	<div class="mt-auto flex flex-col gap-3">
		<Button class="h-12 text-base" disabled={asking} onclick={allow}>{step.allow}</Button>
		<Button variant="ghost" class="h-12 text-base" disabled={asking} onclick={next}>Skip for now</Button>
	</div>
</div>
