(async () => {
  const { cfg, invoke, sleep, mark: note, text, until, go, started, fail } = window.AROH_LIB;
  const mark = async (m) => {
    await note(m);
    if (m.startsWith('shot ')) await sleep(3000);
  };
  const overflows = () => {
    const hits = [];
    if (document.documentElement.scrollWidth > innerWidth + 1) hits.push(`html ${document.documentElement.scrollWidth}>${innerWidth}`);
    const wide = [...document.querySelectorAll('main *')].filter((el) => el.getBoundingClientRect().right > innerWidth + 1 && !el.closest('.overflow-x-auto'));
    if (wide.length) hits.push(`wide ${wide[0].tagName.toLowerCase()}`);
    return hits;
  };
  const check = async (name) => {
    const hits = overflows();
    await invoke('app_dev_mark', { mark: `overflow ${name} ${hits.length} ${hits[0] ?? ''}`.slice(0, 80) });
  };
  const status = () => document.querySelector('[data-testid="replan-status"]');

  try {
    await started();
    await invoke('app_dev_dashboard_seed');
    const { data } = await import('/src/lib/app/data.svelte.ts');
    const client = await import('/src/lib/app/client.ts');
    const { editStatus } = await import('/src/lib/app/replan.svelte.ts');
    await data.load();
    const now = Date.now();
    const soon = data.snapshot.actions
      .filter((a) => a.status === 'planned' && !a.deleted && Date.parse(a.start_at) > now)
      .sort((a, b) => Date.parse(a.start_at) - Date.parse(b.start_at));
    const target = soon[0];
    const moved = soon.find((a) => a.id !== target.id && a.goal_id !== target.goal_id) ?? soon[1];
    await client.editAction(moved.id, target.start_at, target.end_at);
    editStatus.track(moved.id);
    await data.load();

    await go('/calendar');
    await until('edit status', () => status() !== null, 15000);
    await until('overlap mark', () => document.querySelector('[data-overlap]') !== null, 15000);
    await until('not started reason', () => status().dataset.phase === 'not_started', 15000);
    await invoke('app_dev_mark', { mark: `phase ${status().dataset.phase}` });
    await invoke('app_dev_mark', { mark: `overlaps ${[...document.querySelectorAll('[data-overlap]')].filter((el) => el.getClientRects().length).length}` });
    await check('calendar');
    await mark(`shot ${cfg.prefix}-01-calendar-not-started`);
    const marked = [...document.querySelectorAll('[data-overlap]')].find((el) => el.getClientRects().length);
    marked.scrollIntoView({ block: 'center' });
    await sleep(800);
    await mark(`shot ${cfg.prefix}-02-calendar-overlap`);

    await go('/today');
    await until('status on Today', () => status() !== null, 15000);
    await check('today');
    await mark(`shot ${cfg.prefix}-03-today-not-started`);

    const phases = [
      ['04-replanning', { kind: 'replanning', text: 'Replanning: Looking at what changed · 0:42', tone: 'planned', runId: 'r1' }],
      ['05-updated', { kind: 'updated', text: 'Updated: moved Read 20 pages of the very long book title nobody can shorten to Tue 15:00, moved Core workout to Wed 07:00, removed Walk.', tone: 'done', runId: 'r1', undo: true }],
      ['06-failed', { kind: 'failed', text: 'Could not replan. The run stopped: the provider refused the key.', tone: 'missed', runId: 'r1' }],
      ['07-needs-you', { kind: 'needs_you', text: 'Replanning needs your OK in Inbox.', tone: 'warn', runId: 'r1' }]
    ];
    editStatus.dismiss();
    for (const [name, phase] of phases) {
      editStatus.phase = phase;
      await sleep(600);
      await check(name);
      await mark(`shot ${cfg.prefix}-${name}`);
    }
    await go('/calendar');
    await sleep(800);
    await check('calendar-long');
    await mark(`shot ${cfg.prefix}-08-calendar-needs-you`);
    status().querySelector('[aria-label="Dismiss status"]').click();
    await sleep(500);
    await invoke('app_dev_mark', { mark: `dismissed ${status() === null}` });
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
