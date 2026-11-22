(async () => {
  const { invoke, sleep, mark, text, until, go, show, openTomorrow, started, signUp, saveKey, planGoal, fail } = window.AROH_LIB;
  const FACT = 'Plan the morning plank in the evening instead.';
  const hour = 3600e3;
  const refresh = async () => {
    await invoke('app_sync');
    await go('/today');
    await go('/inbox');
  };
  const poll = async (what, ok, ms) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      await refresh();
      if (ok()) return;
      await sleep(5000);
    }
    throw new Error(`timed out waiting for ${what}`);
  };
  const section = (label) => document.querySelector(`section[aria-label="${label}"]`);

  try {
    await started();
    if (!(await invoke('app_notification_permission'))) throw new Error('notifications not allowed');
    await signUp('shot 01-settings-account');
    await mark('shot 02-signed-in');
    await saveKey();
    await mark('shot 03-key-saved');
    await planGoal({ questions: 'shot 04-questions', plan: 'shot 05-plan', approved: 'shot 06-approved' });

    await poll('missed-action proposal', () => text().includes('Make-up core workout') && text().includes('Missed or late actions: Six pack by Feb 1'), 240000);
    await mark('shot 07-inbox-missed-trigger');
    section('Insights')?.scrollIntoView({ block: 'start' });
    await mark('shot 08-insight-missed');

    const snap = await invoke('app_snapshot', { from: new Date(Date.now() - 864e5).toISOString(), to: new Date(Date.now() + 15 * 864e5).toISOString() });
    const planks = snap.actions.filter((a) => a.title === 'Morning plank' && !a.locked).sort((a, b) => a.start_at.localeCompare(b.start_at)).slice(0, 3);
    if (planks.length < 3) throw new Error(`only ${planks.length} planks`);
    for (const a of planks) {
      const start = new Date(Date.parse(a.start_at) + 11 * hour).toISOString();
      const end = new Date(Date.parse(a.end_at) + 11 * hour).toISOString();
      const edited = await invoke('app_edit_action', { id: a.id, startAt: start, endAt: end, delete: false });
      if (!edited.locked) throw new Error('edit did not lock');
    }
    await invoke('app_sync');
    await go('/calendar');
    await sleep(1000);
    await openTomorrow();
    await until('edited plank on the calendar', () => text().includes('Morning plank'), 15000);
    await sleep(500);
    show('Morning plank');
    await mark('shot 09-calendar-edited');

    await poll('preference proposal', () => text().includes(FACT) && text().includes('Repeated edits'), 240000);
    section('Suggestions')?.scrollIntoView({ block: 'start' });
    await mark('shot 10-inbox-preference');
    section('Insights')?.scrollIntoView({ block: 'start' });
    await mark('shot 11-insights');

    const apply = [...(section('Suggestions')?.querySelectorAll('button') ?? [])].find((b) => b.textContent.trim() === 'Apply');
    if (!apply) throw new Error('no Apply on the suggestion');
    apply.click();
    await sleep(3000);
    const prefs = (await invoke('app_snapshot', { from: new Date().toISOString(), to: new Date(Date.now() + 864e5).toISOString() })).preferences;
    if (prefs.some((p) => p.fact === FACT)) throw new Error('preference still unconfirmed');
    await invoke('app_sync');
    await go('/today');
    await go('/inbox');
    await mark('shot 12-preference-confirmed');

    const missed = 'Missed or late actions: Six pack by Feb 1';
    const card = [...(section('Insights')?.querySelectorAll('p') ?? [])].find((p) => p.textContent.includes(missed))?.parentElement;
    const dismiss = [...(card?.querySelectorAll('button') ?? [])].find((b) => b.textContent.trim() === 'Dismiss');
    if (!dismiss) throw new Error('no Dismiss on the missed insight');
    dismiss.click();
    await sleep(2000);
    await refresh();
    await sleep(1500);
    if (text().includes(missed)) throw new Error('dismissed insight came back');
    section('Insights')?.scrollIntoView({ block: 'start' });
    await mark('shot 13-insight-dismissed');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
