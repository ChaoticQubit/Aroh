<script lang="ts">
  import MicButton from './MicButton.svelte';
  import type { Speech } from './speech';

  /** Matches `aroh_intake::MAX_RAW_TEXT_BYTES`; longer text is refused before it is sent. */
  const MAX_BYTES = 2000;

  interface Props {
    speech: Speech;
    onSubmit: (text: string) => Promise<void>;
  }

  let { speech, onSubmit }: Props = $props();

  let text = $state('');
  let busy = $state(false);
  let error = $state('');

  const bytes = $derived(new TextEncoder().encode(text).length);
  const tooLong = $derived(bytes > MAX_BYTES);
  const canSend = $derived(text.trim().length > 0 && !tooLong && !busy);

  /** Appends a voice transcript to whatever the owner already typed. */
  function addTranscript(transcript: string) {
    error = '';
    text = text.trim() ? `${text.trim()} ${transcript}` : transcript;
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!canSend) return;
    busy = true;
    error = '';
    try {
      await onSubmit(text.trim());
      text = '';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<form class="intake" onsubmit={submit}>
  <label class="label" for="log-text">What did you have or do?</label>
  <textarea
    id="log-text"
    class="text"
    rows="3"
    placeholder="Two glasses of water and a 20 minute run"
    bind:value={text}
    aria-invalid={tooLong}
    aria-describedby="log-text-help"
    disabled={busy}
  ></textarea>
  <p id="log-text-help" class="help" class:warn={tooLong || error} aria-live="polite">
    {#if tooLong}
      That is too long to log at once. Split it up.
    {:else if error}
      {error}
    {/if}
  </p>
  <div class="row">
    <MicButton {speech} disabled={busy} onTranscript={addTranscript} onError={(m) => (error = m)} />
    <button type="submit" class="send" disabled={!canSend}>{busy ? 'Logging' : 'Log it'}</button>
  </div>
</form>

<style>
  .intake {
    display: grid;
    gap: 8px;
  }

  .label {
    font-size: 17px;
    font-weight: 600;
    color: var(--log-ink);
  }

  .text {
    width: 100%;
    box-sizing: border-box;
    min-height: 96px;
    padding: 12px 14px;
    border: 1px solid var(--log-field-border);
    border-radius: 12px;
    background: var(--log-surface);
    color: var(--log-ink);
    font: inherit;
    font-size: 17px;
    line-height: 1.45;
    resize: vertical;
  }

  .text:focus-visible {
    outline: 3px solid var(--log-accent);
    outline-offset: 1px;
  }

  .help {
    margin: 0;
    min-height: 20px;
    font-size: 15px;
    color: var(--log-muted);
  }

  .help.warn {
    color: var(--log-danger);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .send {
    flex: none;
    min-height: 48px;
    padding: 0 22px;
    border: none;
    border-radius: 999px;
    background: var(--log-accent);
    color: var(--log-on-accent);
    font: inherit;
    font-size: 17px;
    font-weight: 600;
    transition: transform 120ms ease-out;
  }

  .send:active:not(:disabled) {
    transform: scale(0.97);
  }

  .send:disabled {
    opacity: 0.45;
  }

  .send:focus-visible {
    outline: 3px solid var(--log-accent);
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .send {
      transition: none;
    }
  }
</style>
