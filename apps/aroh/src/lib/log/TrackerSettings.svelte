<script lang="ts">
  import { intake } from './api';

  let enabled = $state(true);
  let time = $state('20:00');
  let water = $state<number | null>(null);
  let workout = $state<number | null>(null);
  let food = $state<number | null>(null);
  let message = $state('');
  let busy = $state(false);

  $effect(() => {
    intake
      .getCheckin()
      .then((c) => {
        enabled = c.enabled;
        time = c.time;
      })
      .catch((e) => (message = String(e)));
    intake
      .totals()
      .then((totals) => {
        water = totals.find((t) => t.kind === 'water')?.target ?? null;
        workout = totals.find((t) => t.kind === 'workout')?.target ?? null;
        food = totals.find((t) => t.kind === 'food')?.target ?? null;
      })
      .catch(() => undefined);
  });

  async function save(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    message = '';
    try {
      await intake.setTargets({ water_ml: water, workout_min: workout, food_kcal: food });
      const saved = await intake.setCheckin(enabled, time);
      message = saved.enabled ? `Saved. Check-in daily at ${saved.time}.` : 'Saved. Check-in is off.';
    } catch (e) {
      message = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section aria-labelledby="trackers-heading">
  <h2 id="trackers-heading">Trackers and check-in</h2>
  <form onsubmit={save}>
    <label>Water target (ml)<input type="number" min="0" step="50" inputmode="numeric" bind:value={water} /></label>
    <label>Workout target (min)<input type="number" min="0" step="5" inputmode="numeric" bind:value={workout} /></label>
    <label>Food target (kcal)<input type="number" min="0" step="50" inputmode="numeric" bind:value={food} /></label>
    <label class="row"><input type="checkbox" bind:checked={enabled} />Daily check-in</label>
    <label>Check-in time<input type="time" bind:value={time} disabled={!enabled} required /></label>
    <button type="submit" disabled={busy}>Save</button>
    <p class="status" aria-live="polite">{message}</p>
  </form>
</section>

<style>
  form {
    display: grid;
    gap: var(--space-3);
    max-width: 420px;
  }

  label {
    display: grid;
    gap: var(--space-1);
    font-size: 15px;
    color: var(--ink-muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--ink);
  }

  input[type='number'],
  input[type='time'] {
    min-height: 44px;
    padding: 0 var(--space-2);
    font: inherit;
    font-size: 17px;
    color: var(--ink);
    background: var(--surface-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  input[type='checkbox'] {
    width: 22px;
    height: 22px;
  }

  button {
    min-height: 44px;
    font: inherit;
    font-weight: 600;
    color: var(--accent-ink);
    background: var(--accent);
    border: 0;
    border-radius: var(--radius);
  }

  .status {
    margin: 0;
    min-height: 1.4em;
  }
</style>
