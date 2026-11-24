<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import PersonaScreen from '$lib/ui/screens/PersonaScreen.svelte';
	import { engine } from '$lib/app/engine';
	import type { PersonaView } from '$lib/app/personas';
	import type { CoachFields } from '$lib/ui/screens/types';

	const name = $derived(page.params.name ?? '');
	const back = $derived(`/personas${page.url.search}`);
	let persona = $state<PersonaView | null>(null);
	let status = $state<string | null>('Loading the coach.');
	let busy = $state(false);

	/** Reads the library and picks this coach; returns false when the engine could not answer. */
	async function load(): Promise<boolean> {
		const r = await engine.personas();
		if (!r.ok) {
			status = r.message;
			return false;
		}
		persona = r.value.find((p) => p.name === name) ?? null;
		status = persona ? null : 'This coach is not in the library.';
		return true;
	}

	$effect(() => {
		void name;
		void load();
	});

	/** Copies the built-in into the library; an existing copy is opened instead. */
	async function oncopy() {
		busy = true;
		const r = await engine.copyPersona(name);
		if (r.ok) {
			persona = r.value;
			status = 'Copied. Edit your copy and save.';
		} else if (r.reason === 'exists') {
			if (await load()) status = 'You already have a copy. It is open now.';
		} else {
			status = r.message;
		}
		busy = false;
	}

	/** Saves the edited fields of the owner's copy; the engine checks them before storing. */
	async function onsave(fields: CoachFields) {
		busy = true;
		status = 'Saving.';
		const r = await engine.editPersona(name, fields);
		if (r.ok) {
			persona = r.value;
			status = 'Saved. Later runs use your copy.';
		} else {
			status = r.message;
		}
		busy = false;
	}
</script>

{#if persona}
	<PersonaScreen {persona} {status} {busy} {oncopy} {onsave} onback={() => void goto(back)} />
{:else}
	<div class="flex flex-col gap-3 p-4">
		<button type="button" class="w-fit text-sm text-muted hover:text-fg" onclick={() => void goto(back)}>Back to coaches</button>
		{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
	</div>
{/if}
