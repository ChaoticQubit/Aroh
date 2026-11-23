(async () => {
  const { invoke, sleep, mark: note, text, until, go, started, fail, show } = window.AROH_LIB;
  const mark = async (m) => {
    await note(m);
    if (m.startsWith('shot ')) await sleep(1500);
  };
  const say = (m) => invoke('app_dev_mark', { mark: m.slice(0, 80) });
  const theme = window.AROH_E2E.theme ?? 'light';
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const ymd = (d) => new Intl.DateTimeFormat('en-CA', { year: 'numeric', month: '2-digit', day: '2-digit', timeZone: zone }).format(d);
  const day = (n) => ymd(new Date(Date.now() + n * 864e5));
  const compact = (s) => s.replaceAll('-', '');
  const at = new Date().toISOString();
  const plan = {
    summary: 'One chapter an evening, around your week.',
    timezone: zone,
    items: [
      { title: 'Read chapter 6', start: `${day(1)}T20:30:00`, minutes: 20, rrule: null, notes: null },
      { title: 'Read the next chapter', start: `${day(2)}T18:00:00`, minutes: 20, rrule: `FREQ=DAILY;UNTIL=${compact(day(18))}`, notes: null }
    ]
  };
  const limits = {
    fits: false, tries: 0, problems: [], move_to: null,
    card: { cap: 20, today: day(0), target: day(10), finish: day(19), needed: 380, raise_to: 40, fits_minutes: 200 },
    questions: [
      { id: 'limit_choice', text: 'At 20 min/day you finish on Dec 12, 9 days after your target. What should change?', kind: 'choice', choices: ['Move the date to Dec 12', 'Raise the daily minutes', 'Lower the target'], unit: null, min: null, max: null, lasting: null, known: null },
      { id: 'new_minutes', text: 'If you raise them, how many minutes a day?', kind: 'number', choices: [], unit: 'minutes per day', min: 21, max: 960, lasting: null, known: null }
    ]
  };
  let goals = [];
  const runOf = (goal) => {
    const i = goals.findIndex((g) => g.id === goal);
    if (i < 0 || i > 1) return null;
    return { run_id: `0000000${i}-0000-4000-8000-000000000000`, goal_id: goal, workflow: 'new-goal', state: 'waiting_human', created_at: at, updated_at: at };
  };
  const hook = async () => {
    const { engine } = await import('/src/lib/app/engine.ts');
    engine.runs = async (f = {}) => ({ ok: true, value: f.goalId && runOf(f.goalId) ? [runOf(f.goalId)] : [] });
    engine.run = async (id) => {
      const i = Number(id[7]);
      const goal = goals[i]?.id;
      const waiting = i === 0 ? [{ node: 'approve', ask: 'approval', prompt: 'Approve this plan?', data: { plan } }] : [{ node: 'limit_choice', ask: 'question', prompt: 'Pick what should change.', data: { limits } }];
      return { ok: true, value: { run_id: id, goal_id: goal, workflow: 'new-goal', status: { status: 'waiting_human', reason: { kind: 'human', node: waiting[0].node } }, waiting, events: [{ seq: 0, at_ms: 1, body: { type: 'run_started' } }], usage: { cost_usd_micros: 30000 } } };
    };
  };
  const main = () => document.querySelector('main');
  const shot = async (name) => {
    await sleep(800);
    await mark(`shot ${theme}/${name}`);
  };
  try {
    await started();
    await say(`seeded ${await invoke('app_dev_seed')}`);
    const { data } = await import('/src/lib/app/data.svelte.ts');
    await data.load();
    goals = data.snapshot?.goals ?? [];
    await hook();
    await go('/today');
    await shot('01-today-root');
    await go('/dashboard');
    await shot('02-dashboard-tab');

    await go(`/goals/${goals[0].id}`);
    await until('plan card', () => text().includes('Approve plan'), 20000);
    show('Approve plan');
    await shot('03-plan-card');
    const block = [...document.querySelectorAll('button[aria-label^="Read chapter 6"]')][0];
    block?.click();
    show('Approve plan');
    await shot('04-plan-tap-session');
    document.querySelector('button[aria-label="Next week"]')?.click();
    await sleep(400);
    show('Approve plan');
    await shot('05-plan-next-week');

    await go(`/goals/${goals[1].id}`);
    await until('cant fit card', () => text().includes('Does not fit'), 20000);
    [...document.querySelectorAll('button[role="radio"]')].find((b) => b.textContent.includes('Raise'))?.click();
    await sleep(500);
    show('Plan with this');
    await shot('06-cant-fit-raise');

    await go('/today');
    const m = main();
    m.scrollTop = Math.min(300, m.scrollHeight - m.clientHeight);
    const kept = Math.round(m.scrollTop);
    await go(`/goals/${goals[0].id}`);
    await sleep(800);
    const backBtn = document.querySelector('header button[aria-label^="Back to"]');
    await say(`back label ${backBtn?.getAttribute('aria-label')}`);
    await shot('07-goal-back-to-today');
    backBtn?.click();
    await until('back on today', () => location.pathname === '/today', 10000);
    await sleep(800);
    await say(`back path ${location.pathname} scroll ${Math.round(main().scrollTop)} of kept ${kept}`);

    await go(`/goals/${goals[0].id}`);
    await until('plan card again', () => text().includes('Approve plan'), 20000);
    main().scrollTop = 400;
    await sleep(300);
    const before = Math.round(main().scrollTop);
    await go(`/personas?goal=${goals[0].id}`);
    await sleep(800);
    document.querySelector('header button[aria-label^="Back to"]')?.click();
    await until('back on goal', () => location.pathname === `/goals/${goals[0].id}`, 10000);
    await sleep(1200);
    await say(`restore goal scroll ${Math.round(main().scrollTop)} of kept ${before}`);

    await go('/settings/places');
    await sleep(800);
    await shot('08-places-back');
    await go('/inbox');
    await shot('09-inbox-from-bell');
    await say('done');
  } catch (e) {
    await fail(e);
  }
})();
