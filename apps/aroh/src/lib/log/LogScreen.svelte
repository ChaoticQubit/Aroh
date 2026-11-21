<script lang="ts">
  import type { Snippet } from 'svelte';
  import EntryList from './EntryList.svelte';
  import IntakeBox from './IntakeBox.svelte';
  import QuickChips from './QuickChips.svelte';
  import TotalsPanel from './TotalsPanel.svelte';
  import { speech as tauriSpeech, type Speech } from './speech';
  import type { Chip, KindTotal, TrackerEntry } from './types';

  /**
   * The phone-first /log screen. Data arrives through props and callbacks so the route
   * can wire it to `commands/intake.rs`. `trackers` is where the plugins pod mounts
   * `<Slot name="log.trackers">`; `speech` defaults to the Tauri `aroh-speech` adapter.
   */
  interface Props {
    entries: readonly TrackerEntry[];
    totals: readonly KindTotal[];
    onSubmitText: (text: string) => Promise<void>;
    onChip: (chip: string) => Promise<void>;
    chips?: readonly Chip[];
    speech?: Speech;
    trackers?: Snippet;
  }

  let { entries, totals, onSubmitText, onChip, chips, speech = tauriSpeech, trackers }: Props = $props();

  let chipError = $state('');

  async function logChip(chip: string) {
    chipError = '';
    try {
      await onChip(chip);
    } catch (e) {
      chipError = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="log">
  <h1 class="title">Log</h1>

  <IntakeBox {speech} onSubmit={onSubmitText} />

  <div class="quick">
    <QuickChips {chips} onChip={logChip} />
    {#if chipError}<p class="error" role="alert">{chipError}</p>{/if}
  </div>

  <TotalsPanel {totals} />

  <EntryList {entries} />

  <section class="trackers" data-slot="log.trackers" aria-label="Trackers">
    {@render trackers?.()}
  </section>
</div>

<style>
  .log {
    --log-ground: var(--aroh-ground, #f4f4f1);
    --log-surface: var(--aroh-surface, #ffffff);
    --log-surface-2: var(--aroh-surface-2, #e8e8e3);
    --log-surface-3: var(--aroh-surface-3, #dcdcd5);
    --log-field-border: var(--aroh-field-border, #c9c9c1);
    --log-ink: var(--aroh-ink, #151614);
    --log-muted: var(--aroh-muted, #5a5c56);
    --log-accent: var(--aroh-accent, #1d6b4e);
    --log-on-accent: var(--aroh-on-accent, #ffffff);
    --log-danger: var(--aroh-danger, #a3261b);

    box-sizing: border-box;
    max-width: 560px;
    margin: 0 auto;
    padding: 0 0 24px;
    display: grid;
    align-content: start;
    gap: 28px;
    background: var(--log-ground);
    color: var(--log-ink);
    font-family: var(--aroh-font, -apple-system, system-ui, sans-serif);
    font-size: 17px;
    line-height: 1.45;
    -webkit-text-size-adjust: 100%;
  }

  .title {
    margin: 0;
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.05;
  }

  .quick {
    display: grid;
    gap: 8px;
  }

  .error {
    margin: 0;
    font-size: 15px;
    color: var(--log-danger);
  }

  .trackers:empty {
    display: none;
  }

  @media (prefers-color-scheme: dark) {
    .log {
      --log-ground: var(--aroh-ground, #121311);
      --log-surface: var(--aroh-surface, #1c1d1b);
      --log-surface-2: var(--aroh-surface-2, #2a2b28);
      --log-surface-3: var(--aroh-surface-3, #363733);
      --log-field-border: var(--aroh-field-border, #45463f);
      --log-ink: var(--aroh-ink, #f1f1ec);
      --log-muted: var(--aroh-muted, #a9aaa2);
      --log-accent: var(--aroh-accent, #4fb889);
      --log-on-accent: var(--aroh-on-accent, #0b1f16);
      --log-danger: var(--aroh-danger, #ff8a7a);
    }
  }
</style>
