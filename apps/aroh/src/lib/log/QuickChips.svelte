<script lang="ts">
  import { DEFAULT_CHIPS, type Chip } from './types';

  interface Props {
    chips?: readonly Chip[];
    onChip: (chip: string) => Promise<void>;
  }

  let { chips = DEFAULT_CHIPS, onChip }: Props = $props();

  let pending: string | null = $state(null);

  async function tap(chip: string) {
    if (pending) return;
    pending = chip;
    try {
      await onChip(chip);
    } finally {
      pending = null;
    }
  }
</script>

<div class="chips" role="group" aria-label="Quick log">
  {#each chips as c (c.chip)}
    <button
      type="button"
      class="chip"
      aria-label={`Log ${c.label.toLowerCase()}`}
      aria-busy={pending === c.chip}
      disabled={pending !== null}
      onclick={() => tap(c.chip)}
    >
      {c.label}
    </button>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    min-height: 44px;
    padding: 0 16px;
    border: none;
    border-radius: 999px;
    background: var(--log-surface-2);
    color: var(--log-ink);
    font: inherit;
    font-size: 16px;
    transition: transform 120ms ease-out, background-color 120ms ease-out;
  }

  .chip:active:not(:disabled) {
    transform: scale(0.96);
    background: var(--log-surface-3);
  }

  .chip:disabled {
    opacity: 0.6;
  }

  .chip:focus-visible {
    outline: 3px solid var(--log-accent);
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .chip {
      transition: none;
    }
  }
</style>
