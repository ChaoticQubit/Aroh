<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import ReplanReview from '$lib/replan/ReplanReview.svelte';
  import { applyReplan, listProposed, rejectReplan } from '$lib/replan/api';
  import type { StoredReplan } from '$lib/replan/types';

  interface Named {
    id: string;
    title: string;
  }

  let replans = $state<StoredReplan[]>([]);
  let titles = $state<Record<string, string>>({});
  let goalTitles = $state<Record<string, string>>({});
  let loading = $state(true);
  let busy = $state(false);
  let error = $state('');

  const byId = (rows: Named[]) => Object.fromEntries(rows.map((r) => [r.id, r.title]));

  async function load() {
    try {
      const [list, actions, goals] = await Promise.all([
        listProposed(),
        invoke<Named[]>('list_actions', { goalId: null }),
        invoke<Named[]>('list_goals')
      ]);
      replans = list;
      titles = byId(actions);
      goalTitles = byId(goals);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function act(run: (id: string) => Promise<void>, id: string) {
    busy = true;
    error = '';
    try {
      await run(id);
    } catch (e) {
      error = String(e);
    }
    await load();
    busy = false;
  }

  $effect(() => {
    load();
  });

  const current = $derived(replans[0]);
</script>

<h1>Replan review</h1>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

{#if loading}
  <p class="muted">Loading</p>
{:else if current}
  <ReplanReview
    proposal={current.proposal}
    {titles}
    {goalTitles}
    {busy}
    onapply={() => act(applyReplan, current.id)}
    onreject={() => act(rejectReplan, current.id)}
  />
  {#if replans.length > 1}
    <p class="muted">{replans.length - 1} more waiting.</p>
  {/if}
{:else}
  <p class="muted">Nothing to review. Your calendar is on track.</p>
{/if}

<style>
  h1 {
    margin: 0 0 var(--space-3);
    font-size: 1.5rem;
  }
  .muted {
    color: var(--ink-muted);
  }
  .error {
    color: var(--danger);
    overflow-wrap: anywhere;
  }
</style>
