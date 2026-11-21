<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { goto } from '$app/navigation';
  import UndoToast from './UndoToast.svelte';
  import { REPLAN_APPLIED, REPLAN_PROPOSED, checkMissed, undoReplan } from './api';
  import type { StoredReplan } from './types';

  let applied = $state<StoredReplan | null>(null);
  let error = $state('');

  $effect(() => {
    let stopped = false;
    const offs = [
      listen<StoredReplan>(REPLAN_APPLIED, (e) => (applied = e.payload)),
      listen<StoredReplan>(REPLAN_PROPOSED, () => goto('/replan'))
    ];
    Promise.all(offs)
      .then(() => (stopped ? undefined : checkMissed()))
      .catch((e) => (error = String(e)));
    return () => {
      stopped = true;
      offs.forEach((off) => off.then((f) => f()));
    };
  });

  async function onundo() {
    const id = applied?.id;
    applied = null;
    if (id) await undoReplan(id).catch((e) => (error = String(e)));
  }
</script>

{#if applied}
  <UndoToast diff={applied.proposal.diff} {onundo} ondismiss={() => (applied = null)} />
{/if}
{#if error}
  <p class="sr" role="alert">Replan failed: {error}</p>
{/if}

<style>
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
