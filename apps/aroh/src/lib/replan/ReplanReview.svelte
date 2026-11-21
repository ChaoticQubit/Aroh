<script lang="ts">
  import { slotLabel, type Proposal } from './types';

  interface Props {
    proposal: Proposal;
    titles?: Record<string, string>;
    goalTitles?: Record<string, string>;
    busy?: boolean;
    onapply: () => void;
    onreject: () => void;
  }

  let { proposal, titles = {}, goalTitles = {}, busy = false, onapply, onreject }: Props = $props();

  const verbs = { move: 'Moved', resize: 'Resized', create: 'New', delete: 'Removed' } as const;
</script>

<section class="review" aria-labelledby="replan-title">
  <h2 id="replan-title">Review replan</h2>

  {#if proposal.infeasible.length > 0}
    <div class="warning" role="alert">
      {#each proposal.infeasible as item (item.goal_id)}
        <p>
          {goalTitles[item.goal_id] ?? item.goal_id}: {item.unplaced_minutes} min do not fit before the target
          date.
        </p>
      {/each}
    </div>
  {/if}

  {#if proposal.diff.length === 0}
    <p class="empty">No changes to the calendar.</p>
  {:else}
    <ul>
      {#each proposal.diff as op (op.action_id)}
        <li>
          <p class="title"><span class="verb">{verbs[op.op]}</span> {op.title ?? titles[op.action_id] ?? op.action_id}</p>
          {#if op.before}<p class="before"><span class="sr">Before: </span>{slotLabel(op.before)}</p>{/if}
          {#if op.after}<p class="after"><span class="sr">After: </span>{slotLabel(op.after)}</p>{/if}
        </li>
      {/each}
    </ul>
  {/if}

  <div class="actions">
    <button type="button" class="reject" disabled={busy} onclick={onreject}>Reject</button>
    <button type="button" class="apply" disabled={busy || proposal.diff.length === 0} onclick={onapply}>Apply</button>
  </div>
</section>

<style>
  .review {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 640px;
    margin: 0 auto;
  }
  h2 {
    margin: 0;
    font-size: 1.25rem;
  }
  .warning {
    padding: 12px;
    border-radius: 12px;
    background: var(--surface-raised);
    border: 1px solid var(--danger);
    color: var(--danger);
  }
  .warning p,
  li p {
    margin: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    padding: 12px;
    border-radius: 12px;
    background: var(--surface-raised);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .verb {
    font-weight: 500;
    color: var(--ink-muted);
  }
  .before {
    color: var(--ink-muted);
    text-decoration: line-through;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .empty {
    color: var(--ink-muted);
  }
  .actions {
    position: sticky;
    bottom: 0;
    display: flex;
    gap: 8px;
    padding-top: 8px;
    background: inherit;
  }
  button {
    flex: 1;
    min-height: 44px;
    border: none;
    border-radius: 12px;
    font: inherit;
    font-weight: 600;
  }
  .apply {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .reject {
    background: var(--line);
    color: var(--ink);
  }
  button:disabled {
    opacity: 0.5;
  }
  button:focus-visible {
    outline: 3px solid var(--accent);
    outline-offset: 2px;
  }
</style>
