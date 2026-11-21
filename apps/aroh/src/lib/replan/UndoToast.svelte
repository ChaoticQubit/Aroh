<script lang="ts">
  import type { DiffOp } from './types';

  interface Props {
    diff: DiffOp[];
    timeoutMs?: number;
    onundo: () => void;
    ondismiss: () => void;
  }

  let { diff, timeoutMs = 8000, onundo, ondismiss }: Props = $props();

  const summary = $derived.by(() => {
    const moved = diff.filter((d) => d.op === 'move').length;
    const resized = diff.filter((d) => d.op === 'resize').length;
    const parts = [moved && `${moved} moved`, resized && `${resized} resized`].filter(Boolean);
    return `Calendar adjusted: ${parts.join(', ') || 'no changes'}.`;
  });

  $effect(() => {
    const timer = setTimeout(ondismiss, timeoutMs);
    return () => clearTimeout(timer);
  });
</script>

<div class="toast" role="status" aria-live="polite">
  <p>{summary}</p>
  <button type="button" onclick={onundo}>Undo</button>
</div>

<style>
  .toast {
    position: fixed;
    left: max(12px, env(safe-area-inset-left));
    right: max(12px, env(safe-area-inset-right));
    bottom: calc(12px + env(safe-area-inset-bottom));
    max-width: 640px;
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px 8px 16px;
    border-radius: 14px;
    background: #1f1f24;
    color: #fff;
  }
  p {
    flex: 1;
    margin: 0;
  }
  button {
    min-height: 44px;
    min-width: 44px;
    padding: 0 16px;
    border: none;
    border-radius: 10px;
    background: transparent;
    color: #8fb0ff;
    font: inherit;
    font-weight: 600;
  }
  button:focus-visible {
    outline: 3px solid #8fb0ff;
  }
</style>
