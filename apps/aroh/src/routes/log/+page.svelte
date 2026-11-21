<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { page } from '$app/state';
  import LogScreen from '../../lib/log/LogScreen.svelte';
  import { intake } from '../../lib/log/api';
  import type { KindTotal, TrackerEntry } from '../../lib/log/types';
  import Slot from '../../lib/plugins/Slot.svelte';
  import type { QueryData, RenderModel } from '../../lib/plugins/types';

  let entries = $state<TrackerEntry[]>([]);
  let totals = $state<KindTotal[]>([]);
  let loadError = $state('');
  let question = $state('');
  let model = $state<RenderModel | null>(null);
  let data = $state<QueryData>({});

  async function refresh() {
    try {
      [entries, totals] = await Promise.all([intake.listToday(), intake.totals()]);
      loadError = '';
    } catch (e) {
      loadError = String(e);
    }
    try {
      [model, data] = await Promise.all([
        invoke<RenderModel>('plugins_model'),
        invoke<QueryData>('plugins_data', { goalId: null }),
      ]);
    } catch {
      model = null;
    }
  }

  async function submitText(text: string) {
    await intake.logText(text);
    question = '';
    await refresh();
  }

  async function logChip(chip: string) {
    await intake.logChip(chip);
    await refresh();
  }

  function logTracker(tracker: string, amount: number) {
    const decl = model?.trackers.find((t) => t.id === tracker);
    if (!decl || (decl.kind !== 'water' && decl.kind !== 'workout')) return;
    logChip(`${decl.kind} ${amount}${decl.unit}`).catch((e) => (loadError = String(e)));
  }

  $effect(() => {
    refresh();
  });

  $effect(() => {
    if (page.url.searchParams.get('checkin') !== '1') return;
    intake
      .checkinQuestion()
      .then((q) => (question = q))
      .catch(() => (question = 'How is today going? Log anything you have not tracked yet.'));
  });
</script>

{#if question}
  <aside class="checkin" aria-live="polite">
    <span class="label">Check-in</span>
    <p>{question}</p>
  </aside>
{/if}

{#if loadError}<p class="error" role="alert">{loadError}</p>{/if}

<LogScreen {entries} {totals} onSubmitText={submitText} onChip={logChip}>
  {#snippet trackers()}
    {#if model}
      <Slot name="log.trackers" {model} {data} onlog={logTracker} />
    {/if}
  {/snippet}
</LogScreen>

<style>
  .checkin {
    max-width: 560px;
    margin: 0 auto 20px;
    padding: 14px 16px;
    border-left: 3px solid var(--agent);
    background: var(--surface-raised);
    border-radius: var(--radius);
  }

  .label {
    font-size: 13px;
    font-weight: 600;
    color: var(--agent);
  }

  .checkin p {
    margin: 4px 0 0;
    font-size: 17px;
  }

  .error {
    max-width: 560px;
    margin: 0 auto;
    color: var(--danger);
  }
</style>
