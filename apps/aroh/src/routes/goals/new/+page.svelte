<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import NewGoalForm from '$lib/ui/components/goal/NewGoalForm.svelte';
	import type { Attachment } from '$lib/ui/components/goal/Attachments.svelte';
	import { pendingAttachment, uploadFailed } from '$lib/app/files';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { engine } from '$lib/app/engine';
	import { agentDefaults, blankDraft, startBody, withGuess } from '$lib/app/newgoal';
	import { categoryChoices, SLOT_HUES } from '$lib/app/views';
	import { deviceZone, getClock } from '$lib/ui/clock';
	import { formatYmd } from '$lib/ui/format';
	import type { CategorySlot } from '$lib/ui/contract';

	const GUESS_MS = 400;
	const clock = getClock();
	const s = $derived(data.snapshot);
	let draft = $state({ ...blankDraft(agentDefaults(data.snapshot?.settings.agent_defaults)), title: page.url.searchParams.get('title')?.slice(0, 200) ?? '' });
	let detected = $state(false);
	let creating = $state(false);
	let error = $state<string | null>(null);
	let defaultLimit = $state(5);
	let upload = $state(false);
	let files = $state<Attachment[]>([]);
	const goalId = crypto.randomUUID();
	const picked = new Map<string, File>();
	const links = new Map<string, string>();
	const stored = new Set<string>();

	onMount(async () => {
		const limit = await engine.runLimit();
		if (limit.ok) defaultLimit = limit.value.minutes;
	});
	onMount(async () => (upload = await client.filesAvailable()));

	/** Lists files picked on the form; they upload on Create. */
	function onfiles(list: File[]) {
		const added = list.map((file, i) => {
			const id = `p${files.length + i}-${file.name}`;
			picked.set(id, file);
			return pendingAttachment(id, file);
		});
		files = [...files, ...added];
	}

	/** Lists a typed link; it is stored on Create. */
	function onlink(url: string) {
		const id = `l${files.length}-${url}`;
		links.set(id, url);
		files = [...files, { id, kind: 'link', name: url, detail: 'Link · agents read it' }];
	}

	/** Sends a failed upload again; on success it shows as stored. */
	async function onretry(id: string) {
		const file = picked.get(id);
		if (!file) return;
		try {
			await client.attachFile(goalId, file);
			stored.add(id);
			files = files.map((x) => (x.id === id ? { ...pendingAttachment(id, file), detail: 'Uploaded' } : x));
			error = null;
		} catch (e) {
			error = `${file.name}: ${String(e)}`;
		}
	}

	/** Drops an attachment that is not stored yet. */
	function onremove(id: string) {
		picked.delete(id);
		links.delete(id);
		files = files.filter((f) => f.id !== id);
	}

	/**
	 * Stores every attachment not stored yet under the new goal's id, so research reads them;
	 * a retry after a failed upload skips the ones already stored. Returns the refusal, if any.
	 */
	async function storeFiles(): Promise<string | null> {
		for (const f of files) {
			if (stored.has(f.id)) continue;
			try {
				const file = picked.get(f.id);
				const url = links.get(f.id);
				if (file) await client.attachFile(goalId, file);
				else if (url) await client.attachLink(goalId, url);
				stored.add(f.id);
			} catch (e) {
				files = files.map((x) => (x.id === f.id ? uploadFailed(x) : x));
				return `${f.name}: ${String(e)}`;
			}
		}
		if (files.length) await client.sync().catch(() => null);
		return null;
	}

	$effect(() => {
		const title = draft.title.trim();
		const description = draft.description.trim() || null;
		if (!title || draft.categoryPicked) return;
		const timer = setTimeout(async () => {
			const guessed = await client.guessCategory(title, description).catch(() => null);
			if (draft.title.trim() !== title || draft.categoryPicked) return;
			draft.categoryId = withGuess(draft, guessed).categoryId;
			detected = guessed !== null;
		}, GUESS_MS);
		return () => clearTimeout(timer);
	});

	/** Adds the owner's category on the hue of `slot` and picks it; returns the refusal to show, if any. */
	async function addCategory(name: string, slot: CategorySlot): Promise<string | null> {
		try {
			const row = await client.addCategory(name, SLOT_HUES[slot]);
			await data.load();
			draft = { ...draft, categoryId: row.id, categoryPicked: true };
			return null;
		} catch (e) {
			return String(e);
		}
	}

	/** Stores the attachments, creates the goal on the engine under the same id, then opens its goal card. */
	async function create() {
		creating = true;
		error = null;
		const refused = await storeFiles();
		if (refused) {
			creating = false;
			error = refused;
			return;
		}
		const started = await engine.startGoal({ ...startBody(draft, deviceZone()), goal_id: goalId });
		creating = false;
		if (!started.ok) {
			error = started.message;
			return;
		}
		await client.sync().catch(() => null);
		await data.load();
		await goto(`/goals/${started.value.goal_id}`);
	}
</script>

{#if s}
	<NewGoalForm bind:draft categories={categoryChoices(s)} today={formatYmd(clock.now, clock.zone)} {defaultLimit} {detected} {creating} {error} attach {upload} {files} {onfiles} {onlink} {onretry} {onremove} onaddcategory={addCategory} oncreate={create} />
{/if}
