(async () => {
  const { cfg, invoke, sleep, mark: note, text, until, go, started, fail } = window.AROH_LIB;
  const mark = async (m) => {
    await note(m);
    if (m.startsWith('shot ')) await sleep(3000);
  };
  const LONG = 'with the extra long name that keeps going past the phone edge';
  const WORD = 'Supercalifragilisticexpialidocious-checklist';
  const ROUTES = ['/today', '/calendar', '/goals', 'goal', '/dashboard', '/inbox', '/log', '/settings', '/settings/location', '/settings/location/log', '/settings/places', '/settings/profile', '/personas'];
  let goals = [];

  const lengthen = (s) => {
    if (!s) return s;
    goals = s.goals ?? [];
    for (const g of s.goals ?? []) if (!g.title.includes(LONG)) g.title = `${g.title} ${LONG}`;
    (s.actions ?? []).forEach((a, i) => {
      if (a.title.includes(LONG) || a.title.includes(WORD)) return;
      a.title = i % 3 === 0 ? `${WORD} ${a.title}` : `${a.title} ${LONG}`;
    });
    return s;
  };
  const runs = () => {
    const at = new Date().toISOString();
    const run = (i, state) => ({ run_id: `0000000${i}-0000-4000-8000-000000000000`, goal_id: goals[i % Math.max(1, goals.length)]?.id ?? null, workflow: i === 2 ? 'replan' : 'new-goal', state, created_at: at, updated_at: at });
    return [run(0, 'failed'), run(1, 'failed'), run(2, 'running')];
  };
  const hookApp = async () => {
    const { data } = await import('/src/lib/app/data.svelte.ts');
    const { engine } = await import('/src/lib/app/engine.ts');
    const load = data.load.bind(data);
    data.load = async () => {
      await load();
      data.snapshot = lengthen(data.snapshot ? JSON.parse(JSON.stringify(data.snapshot)) : null);
    };
    engine.runs = async (f = {}) => ({ ok: true, value: runs().filter((r) => (!f.goalId || r.goal_id === f.goalId) && (!f.state || r.state === f.state)) });
    engine.run = async (id) => {
      const r = runs().find((x) => x.run_id === id);
      const events = [{ seq: 0, at_ms: 1, body: { type: 'run_started' } }, { seq: 1, at_ms: 2, body: { type: 'step_started', step: 'intake' } }, { seq: 2, at_ms: 3, body: { type: 'step_finished', step: 'intake' } }, { seq: 3, at_ms: 4, body: { type: 'step_started', step: 'research' } }];
      return r ? { ok: true, value: { run_id: id, goal_id: r.goal_id, workflow: r.workflow, status: { status: 'failed', reason: { kind: 'time_limit', limit_ms: 300000 } }, waiting: [], events, usage: { cost_usd_micros: 40000 } } } : { ok: false, reason: 'error', message: 'no run' };
    };
    engine.insights = async () => ({ ok: true, value: [] });
    return data;
  };

  const shell = () => document.querySelector('.fixed.inset-x-0.top-0');
  const overflows = () => {
    const hits = [];
    const root = document.documentElement;
    if (root.scrollWidth > innerWidth + 1) hits.push(`html ${root.scrollWidth}>${innerWidth}`);
    for (const el of document.querySelectorAll('body *')) {
      if (String(el.className).includes('overflow-x-auto')) continue;
      const x = getComputedStyle(el).overflowX;
      if ((x === 'auto' || x === 'scroll') && el.scrollWidth > el.clientWidth + 1) hits.push(`${el.tagName.toLowerCase()} ${el.scrollWidth}>${el.clientWidth}`);
    }
    return hits;
  };
  const check = async (route, width) => {
    const path = route === 'goal' ? `/goals/${goals[0].id}` : route;
    await go(path);
    await sleep(1500);
    if (['/today', '/goals', 'goal'].includes(route)) await until(`long title on ${route}`, () => text().includes(LONG), 15000);
    const hits = overflows();
    const name = route === 'goal' ? 'goal' : route.slice(1).replaceAll('/', '-');
    if (hits.length) await invoke('app_dev_mark', { mark: `overflow ${width} ${name} ${hits[0]}`.slice(0, 80) });
    if (hits.length && width === innerWidth) {
      const wide = [...document.querySelectorAll('main *')].filter((el) => el.getBoundingClientRect().width > innerWidth + 1);
      const top = wide[0];
      if (top) await invoke('app_dev_mark', { mark: `widest ${name} ${top.tagName.toLowerCase()}.${String(top.className).replace(/[^\x20-\x7e]/g, '?')}`.slice(0, 80) });
    }
    return { name, hits };
  };

  try {
    await started();
    const data = await hookApp();
    await invoke('app_dev_seed');
    await data.load();
    await until('seeded goals', () => goals.length > 0, 30000);
    let bad = 0;
    for (const width of [0, 375]) {
      const s = shell();
      const kept = s?.getAttribute('style') ?? '';
      if (width && s) {
        s.style.right = 'auto';
        s.style.width = `${width}px`;
      }
      for (const route of ROUTES) {
        const { name, hits } = await check(route, width || innerWidth);
        bad += hits.length ? 1 : 0;
        if (!width && name === 'inbox') document.querySelector('section[aria-label="Runs in progress"]')?.scrollIntoView({ block: 'start' });
        if (!width && cfg.shots?.includes(name)) await mark(`shot ${cfg.prefix}-${name}`);
      }
      if (s) s.setAttribute('style', kept);
    }
    await invoke('app_dev_mark', { mark: `overflow-routes ${bad}` });

    await go(`/goals/${goals[0].id}`);
    await until('goal screen', () => text().includes(LONG), 15000);
    const main = document.querySelector('main');
    main.scrollTop = main.scrollHeight;
    await sleep(800);
    const errors = [];
    const keep = (e) => errors.push(String(e?.reason?.stack ?? e?.reason ?? e?.error?.stack ?? e?.message ?? e));
    addEventListener('error', keep);
    addEventListener('unhandledrejection', keep);
    const log = console.error;
    console.error = (...a) => (errors.push(a.map(String).join(' ')), log(...a));
    [...document.querySelectorAll('nav[aria-label="Main"] button')].find((b) => b.textContent.trim() === 'Goals').click();
    await sleep(2500);
    console.error = log;
    const box = document.querySelector('main').getBoundingClientRect();
    const row = [...document.querySelectorAll('main button')].find((b) => b.textContent.includes(LONG))?.getBoundingClientRect();
    const seen = !!row && row.bottom > box.top && row.top < box.bottom;
    const blank = location.pathname !== '/goals' || !seen;
    await invoke('app_dev_mark', { mark: `goals-scroll ${Math.round(document.querySelector('main').scrollTop)} of ${document.querySelector('main').scrollHeight}` });
    for (const e of errors.slice(0, 3)) for (let i = 0; i < Math.min(e.length, 320); i += 70) await invoke('app_dev_mark', { mark: `err ${e.slice(i, i + 70).replace(/[^\x20-\x7e]/g, '?')}` });
    await invoke('app_dev_mark', { mark: `goals-tab ${blank ? 'blank' : 'ok'} ${location.pathname}` });
    await mark(`shot ${cfg.prefix}-goals-tab-from-goal`);

    await go('/today');
    const tick = [...document.querySelectorAll('button[aria-label^="Mark done: "]')][0];
    if (!tick) throw new Error('no action to mark done on Today');
    const ticked = tick.getAttribute('aria-label').slice('Mark done: '.length);
    tick.click();
    await sleep(2500);
    await mark(`shot ${cfg.prefix}-done-today`);
    await go('/calendar');
    await sleep(1500);
    const block = [...document.querySelectorAll('[role=listitem]')].find((b) => b.textContent.includes(ticked) && b.getBoundingClientRect().width > 0);
    if (!block) throw new Error('ticked action not on the calendar');
    const scroller = document.querySelector('main');
    scroller.scrollTop += block.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 160;
    await sleep(800);
    const title = block.querySelector('p');
    const struck = getComputedStyle(title).textDecorationLine.includes('line-through');
    const tinted = !block.getAttribute('style').includes(' 0%,');
    await invoke('app_dev_mark', { mark: `calendar-done ${struck && !tinted ? 'ok' : 'planned-style'} struck=${struck} tinted=${tinted}` });
    await mark(`shot ${cfg.prefix}-done-calendar`);

    const pane = document.querySelector('main [style*="touch-action: pan-y"]');
    const range = () => document.querySelector('p[aria-live="polite"]').textContent.trim();
    const drag = async (x0, y0, x1, y1, hold) => {
      const point = (x, y) => new Touch({ identifier: 1, target: pane, clientX: x, clientY: y, pageX: x, pageY: y });
      const fire = (type, x, y) => pane.dispatchEvent(new TouchEvent(type, { bubbles: true, cancelable: true, touches: type === 'touchend' ? [] : [point(x, y)], changedTouches: [point(x, y)] }));
      fire('touchstart', x0, y0);
      for (let i = 1; i <= 10; i++) {
        fire('touchmove', x0 + ((x1 - x0) * i) / 10, y0 + ((y1 - y0) * i) / 10);
        await sleep(30);
      }
      if (hold) await hold();
      fire('touchend', x1, y1);
      await sleep(900);
    };
    const week0 = range();
    await drag(320, 500, 120, 505, () => mark(`shot ${cfg.prefix}-swipe-mid`));
    const week1 = range();
    await drag(10, 500, 250, 505);
    const edge = range();
    await drag(200, 400, 210, 650);
    const vertical = range();
    await drag(100, 500, 330, 505);
    const back = range();
    const swiped = week1 !== week0 && edge === week1 && vertical === week1 && back === week0;
    await invoke('app_dev_mark', { mark: `swipe ${swiped ? 'ok' : 'broken'}` });
    await invoke('app_dev_mark', { mark: `swipe ${week0} > ${week1} > ${back}`.slice(0, 80) });
    await drag(320, 500, 120, 505);
    await mark(`shot ${cfg.prefix}-swipe-next-week`);
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
