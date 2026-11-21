<script lang="ts">
  import { formatAmount, formatTime } from './format';
  import { KIND_LABEL, type TrackerEntry } from './types';

  interface Props {
    entries: readonly TrackerEntry[];
  }

  let { entries }: Props = $props();

  const shown = $derived(
    entries.filter((e) => !e.deleted).sort((a, b) => b.logged_at.localeCompare(a.logged_at)),
  );
</script>

<section class="entries" aria-labelledby="log-entries-title">
  <h2 id="log-entries-title" class="title">Today</h2>
  {#if shown.length === 0}
    <p class="empty">Nothing logged yet. Type it, tap a chip or hold the mic.</p>
  {:else}
    <ul class="list">
      {#each shown as entry (entry.id)}
        <li class="item">
          <div class="main">
            <span class="kind">{KIND_LABEL[entry.kind]}</span>
            <span class="amount">{formatAmount(entry.amount ?? 0, entry.unit ?? '')}</span>
          </div>
          <div class="meta">
            {#if entry.note}<span class="note">{entry.note}</span>{/if}
            <time datetime={entry.logged_at}>{formatTime(entry.logged_at)}</time>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .entries {
    display: grid;
    gap: 12px;
  }

  .title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--log-ink);
  }

  .empty {
    margin: 0;
    font-size: 16px;
    color: var(--log-muted);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  .item {
    display: grid;
    gap: 2px;
    min-height: 44px;
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--log-surface);
  }

  .main {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 17px;
    color: var(--log-ink);
  }

  .kind {
    font-weight: 600;
  }

  .amount {
    font-variant-numeric: tabular-nums;
  }

  .meta {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 15px;
    color: var(--log-muted);
  }

  .note {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .meta time {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
</style>
