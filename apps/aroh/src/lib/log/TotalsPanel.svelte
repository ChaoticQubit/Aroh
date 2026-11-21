<script lang="ts">
  import { formatAmount } from './format';
  import { KIND_LABEL, type KindTotal } from './types';

  interface Props {
    totals: readonly KindTotal[];
  }

  let { totals }: Props = $props();

  /** Share of the target reached, clamped to 0..1; null when there is no positive target. */
  function share(t: KindTotal): number | null {
    if (t.target === null || t.target <= 0) return null;
    return Math.min(1, Math.max(0, t.total / t.target));
  }
</script>

<section class="totals" aria-labelledby="log-totals-title">
  <h2 id="log-totals-title" class="title">Against your targets</h2>
  <ul class="list">
    {#each totals as t (t.kind)}
      {@const s = share(t)}
      <li class="item">
        <div class="head">
          <span class="kind">{KIND_LABEL[t.kind]}</span>
          <span class="figure">
            {formatAmount(t.total, t.unit)}{#if t.target !== null}<span class="of">{` of ${formatAmount(t.target, t.unit)}`}</span>{/if}
          </span>
        </div>
        {#if s !== null}
          <div
            class="bar"
            role="progressbar"
            aria-label={`${KIND_LABEL[t.kind]} progress`}
            aria-valuemin={0}
            aria-valuemax={t.target}
            aria-valuenow={Math.min(t.total, t.target ?? 0)}
            aria-valuetext={`${formatAmount(t.total, t.unit)} of ${formatAmount(t.target ?? 0, t.unit)}`}
          >
            <div class="fill" style:transform={`scaleX(${s})`}></div>
          </div>
          <p class="note">
            {#if t.remaining !== null && t.remaining > 0}
              {formatAmount(t.remaining, t.unit)} to go
            {:else}
              Target reached
            {/if}
          </p>
        {:else}
          <p class="note">No target set</p>
        {/if}
      </li>
    {/each}
  </ul>
</section>

<style>
  .totals {
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

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 16px;
  }

  .item {
    display: grid;
    gap: 6px;
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    color: var(--log-ink);
  }

  .kind {
    font-size: 17px;
    font-weight: 600;
  }

  .figure {
    font-size: 17px;
    font-variant-numeric: tabular-nums;
  }

  .of {
    color: var(--log-muted);
  }

  .bar {
    height: 10px;
    border-radius: 999px;
    background: var(--log-surface-3);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--log-accent);
    transform-origin: left;
    transition: transform 300ms ease-out;
  }

  .note {
    margin: 0;
    font-size: 15px;
    color: var(--log-muted);
  }

  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
  }
</style>
