<script lang="ts">
  import { SpeechError, type Speech } from './speech';

  /** Press longer than this and release stops listening (hold to talk); shorter is a tap toggle. */
  const HOLD_MS = 350;

  interface Props {
    speech: Speech;
    disabled?: boolean;
    onTranscript: (text: string) => void;
    onError: (message: string) => void;
  }

  let { speech, disabled = false, onTranscript, onError }: Props = $props();

  type Phase = 'idle' | 'starting' | 'listening' | 'stopping';
  let phase = $state<Phase>('idle');
  let partial = $state('');
  let pressedAt: number | null = null;
  let starting: Promise<boolean> | null = null;

  const active = $derived(phase === 'starting' || phase === 'listening');
  const label = $derived(
    phase === 'stopping' ? 'Finishing voice input' : active ? 'Stop voice input' : 'Start voice input',
  );

  /** Starts recognition; resolves true when listening, false when the plugin refused. */
  async function begin(): Promise<boolean> {
    console.assert(phase === 'idle', 'begin called while', phase);
    phase = 'starting';
    partial = '';
    try {
      await speech.start((text) => (partial = text));
      phase = 'listening';
      return true;
    } catch (e) {
      phase = 'idle';
      pressedAt = null;
      onError(e instanceof SpeechError ? e.message : 'Voice input failed.');
      return false;
    }
  }

  /** Stops recognition after any pending start and hands a non-empty transcript to `onTranscript`. */
  async function end(): Promise<void> {
    if (starting && !(await starting)) return;
    if (phase !== 'listening') return;
    phase = 'stopping';
    try {
      const text = (await speech.stop()).trim();
      if (text) onTranscript(text);
    } catch (e) {
      onError(e instanceof SpeechError ? e.message : 'Voice input failed.');
    } finally {
      phase = 'idle';
      partial = '';
    }
  }

  function press(event: PointerEvent) {
    if (disabled || event.button !== 0 || phase !== 'idle') return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    pressedAt = performance.now();
    starting = begin();
  }

  /**
   * Ends a press. A long hold stops listening, unless start is still waiting on the
   * microphone and speech permission prompts: they cancel the touch, so the press then
   * counts as a tap and listening goes on until the next tap.
   */
  function release() {
    if (pressedAt !== null) {
      const held = performance.now() - pressedAt;
      pressedAt = null;
      if (held >= HOLD_MS && phase !== 'starting') void end();
      return;
    }
    if (active) void end();
  }

  function keyboardToggle(event: MouseEvent) {
    if (event.detail !== 0 || disabled) return;
    if (phase === 'idle') starting = begin();
    else if (active) void end();
  }
</script>

<div class="mic">
  <button
    type="button"
    class="button"
    class:active
    aria-label={label}
    aria-pressed={active}
    disabled={disabled || phase === 'stopping'}
    onpointerdown={press}
    onpointerup={release}
    onpointercancel={release}
    onclick={keyboardToggle}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <rect x="9" y="3" width="6" height="11" rx="3" fill="currentColor" />
      <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
    </svg>
  </button>
  <p class="status" aria-live="polite">
    {#if phase === 'starting'}
      Starting. If iPhone asks, allow the microphone and speech recognition.
    {:else if phase === 'listening'}
      {partial || 'Listening. Tap again or let go to finish.'}
    {:else if phase === 'stopping'}
      Finishing
    {/if}
  </p>
</div>

<style>
  .mic {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .button {
    flex: none;
    width: 52px;
    height: 52px;
    border: none;
    border-radius: 50%;
    background: var(--log-surface-2);
    color: var(--log-ink);
    display: grid;
    place-items: center;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    transition: transform 120ms ease-out, background-color 120ms ease-out;
  }

  .button:active {
    transform: scale(0.94);
  }

  .button.active {
    background: var(--log-accent);
    color: var(--log-on-accent);
  }

  .button:disabled {
    opacity: 0.5;
  }

  .button:focus-visible {
    outline: 3px solid var(--log-accent);
    outline-offset: 2px;
  }

  .status {
    margin: 0;
    min-width: 0;
    font-size: 15px;
    line-height: 1.4;
    color: var(--log-muted);
    overflow-wrap: anywhere;
  }

  @media (prefers-reduced-motion: reduce) {
    .button {
      transition: none;
    }
  }
</style>
