<script lang="ts">
	import { untrack } from 'svelte';
	import NewGoalForm, { type CategoryChoice } from '$lib/ui/components/goal/NewGoalForm.svelte';
	import type { Attachment } from '$lib/ui/components/goal/Attachments.svelte';
	import { categories as seeded } from '$lib/ui/mockdata/sixpack';
	import { blankDraft, pickCategory } from '$lib/app/newgoal';
	import type { CategorySlot } from '$lib/ui/contract';
	import { attachments, book } from './data';

	/**
	 * One New Goal frame of the mockup: the promoted form with its own state. `filled` starts from the
	 * sample book goal with its attachments; `adding` opens New category showing `newName`.
	 */
	let { filled = false, adding = false, newName = '' }: { filled?: boolean; adding?: boolean; newName?: string } = $props();

	let categories = $state<CategoryChoice[]>(seeded.map((c) => ({ id: c.name, ...c })));
	const initial = { ...blankDraft(), title: book.title, description: book.description, target: book.target, categoryId: book.category };
	let draft = $state(untrack(() => (filled ? initial : blankDraft())));
	let files = $state<Attachment[]>(untrack(() => (filled ? attachments : [])));

	/** Adds the category to this frame only and picks it. */
	async function add(name: string, slot: CategorySlot): Promise<string | null> {
		categories = [...categories, { id: name, name, slot }];
		draft = pickCategory(draft, name);
		return null;
	}
</script>

<NewGoalForm bind:draft bind:files {categories} today="2026-11-26" detected={filled} attach {adding} {newName} onaddcategory={add} />
