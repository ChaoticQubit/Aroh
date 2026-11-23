(async () => {
  const L = window.AROH_LIB;
  const { sleep, text, until, go, fail, skipSetup } = L;
  const T = window.__TAURI_INTERNALS__;
  const real = T.invoke;
  const called = [];
  const invoke = (cmd, args) => real.call(T, cmd, args ?? {});
  const note = (m) => invoke('app_dev_mark', { mark: String(m).replace(/[^\x20-\x7e]/g, '?').slice(0, 80) }).catch(() => {});
  const result = (name, ok, detail = '') => note(`result ${name} ${ok ? 'pass' : 'fail'} ${detail}`);
  const shot = async (name) => {
    await note(`shot ${name}`);
    await sleep(4500);
  };
  const main = () => document.querySelector('main');
  const ago = (min) => new Date(Date.now() - min * 60000).toISOString();

  const smoke = !!window.AROH_E2E?.smoke;
  const only = window.AROH_E2E?.only ?? [];
  window.alert = (m) => note(`dialog alert ${m}`);
  window.confirm = (m) => (note(`dialog confirm ${m}`), false);
  window.prompt = (m) => (note(`dialog prompt ${m}`), null);
  let six, piano, save, reading, snap, hooked;
  let signedIn = true;
  const setup = async () => {
  const range = { from: new Date(Date.now() - 30 * 864e5).toISOString(), to: new Date(Date.now() + 30 * 864e5).toISOString() };
  snap = await invoke('app_snapshot', range);
  const goal = (prefix) => snap.goals.find((g) => !g.deleted && g.title.startsWith(prefix));
  six = goal('Six pack');
  piano = goal('Learn piano');
  save = goal('Save $10k');
  reading = goal('Read The Joy of X');
  if (!six || !piano || !save || !reading) throw new Error('gate seed goals missing');

  const QUESTIONS = [
    { id: 'days', text: 'How many days a week can you train without dropping it after the first week?', kind: 'number', choices: [], unit: 'days', min: 1, max: 7 },
    { id: 'minutes', text: 'How many minutes can one session take on a normal workday, warm-up included?', kind: 'number', choices: [], unit: 'min', min: 10, max: 120 },
    { id: 'when', text: 'When in the day do you train best?', kind: 'choice', choices: ['Early morning before work', 'Lunch break at the office gym', 'Evening after the kids are asleep'] },
    { id: 'weight', text: 'What is your current body weight?', kind: 'number', choices: [], unit: 'kg', min: 40, max: 200, known: { fact_id: 'f1', value: '82 kg', source: 'Health' } },
    { id: 'limits', text: 'Any injuries, pain or movements your doctor told you to avoid?', kind: 'text', choices: [] },
    { id: 'kit', text: 'What equipment do you have at home, for example dumbbells, a bench or a pull-up bar?', kind: 'text', choices: [] }
  ];
  const run = (id, g, state, at) => ({ run_id: id, goal_id: g?.id ?? null, workflow: 'new-goal', state, created_at: at, updated_at: at });
  const RQ = '0000000a-0000-4000-8000-000000000001';
  const RS = '0000000a-0000-4000-8000-000000000002';
  const RP = '0000000a-0000-4000-8000-000000000003';
  const RW = '0000000a-0000-4000-8000-000000000004';
  const RUNS = [run(RW, reading, 'running', ago(2)), run(RQ, six, 'waiting_human', ago(20)), run(RP, save, 'waiting_human', ago(40)), run(RS, piano, 'failed', ago(60))];
  const t0 = Date.now() - 10 * 60000;
  const events = (done, open) => {
    const out = [{ seq: 0, at_ms: t0, body: { type: 'run_started' } }];
    done.forEach((s, i) => out.push({ seq: out.length, at_ms: t0 + i * 40000, body: { type: 'step_started', step: s } }, { seq: out.length + 1, at_ms: t0 + i * 40000 + 30000, body: { type: 'step_finished', step: s } }));
    if (open) out.push({ seq: out.length, at_ms: Date.now() - 95000, body: { type: 'step_started', step: open } });
    return out;
  };
  const research = { summary: 'Core training three to four times a week plus a modest calorie deficit works for desk workers; visible abs usually need body fat near 12 percent, so the plan pairs short core sessions with a meal check-in and walks after lunch.', sources: [{ title: 'Core training for people who sit all day: a practical guide', url: 'https://example.org/core-training-desk-workers' }, { title: 'How fast can you lose body fat safely', url: 'https://example.org/fat-loss-rate' }] };
  const plan = {
    summary: 'Save $1,250 a month: an automatic transfer on payday, a weekly budget review on Fridays and a takeaway cap of two orders a week until June 30.',
    items: [
      { title: 'Set up the automatic transfer of $1,250 to the high-interest savings account on payday', start: '2026-11-25T09:00:00', minutes: 20, rrule: null, notes: null },
      { title: 'Weekly budget review with the spreadsheet and the takeaway count', start: '2026-11-27T12:00:00', minutes: 30, rrule: 'FREQ=WEEKLY;BYDAY=FR', notes: null },
      { title: 'Cancel the two streaming subscriptions nobody watched this month', start: '2026-11-26T19:00:00', minutes: 15, rrule: null, notes: null }
    ]
  };
  const VIEWS = {
    [RQ]: { run_id: RQ, goal_id: six?.id, workflow: 'new-goal', status: { status: 'waiting_human' }, waiting: [{ node: 'questions', ask: 'question', prompt: 'A few questions so the plan fits your week. Skip any you do not want to answer.', data: { intake: { questions: QUESTIONS } } }], events: events(['intake', 'research']), usage: { cost_usd_micros: 31000 }, research, answers: [] },
    [RS]: { run_id: RS, goal_id: piano?.id, workflow: 'new-goal', status: { status: 'failed', reason: { kind: 'time_limit', limit_ms: 300000 } }, waiting: [], events: events(['intake'], 'research'), usage: { cost_usd_micros: 42000 }, research: null, answers: [] },
    [RP]: { run_id: RP, goal_id: save?.id, workflow: 'new-goal', status: { status: 'waiting_human' }, waiting: [{ node: 'approve', ask: 'approval', prompt: null, data: { plan } }], events: events(['intake', 'research', 'plan']), usage: { cost_usd_micros: 58000 }, research, answers: [{ id: 'income', text: 'What is your monthly take-home pay after tax?', answer: '$4,800' }, { id: 'fixed', text: 'What are your fixed monthly costs?', answer: null }] },
    [RW]: { run_id: RW, goal_id: reading?.id, workflow: 'new-goal', status: { status: 'running' }, waiting: [], events: events(['intake'], 'research'), usage: { cost_usd_micros: 12000 }, research: null, answers: [] }
  };
  const INSIGHTS = [{ id: 'i1', goal_id: six?.id ?? null, run_id: null, source: 'adapt', title: 'Moved Thursday core session to 07:30 because the team stand-up now starts at 07:00', body: 'Your calendar showed a recurring meeting at 07:00 on Thursdays, so the core session moved after it.', state: null, created_at: ago(90) }];
  let limit = 5;
  const coach = (name, display_name, role, teams, builtin) => ({ name, display_name, role, instructions: `${display_name} keeps sessions short on workdays, checks the owner's calendar before suggesting a time, and explains every change in one plain sentence.`, guardrails: ['Never plans more than the daily minutes the owner set', 'Asks before moving a session the owner fixed'], locked: ['Never plans more than the daily minutes the owner set'], teams, source: builtin ? 'builtin' : 'library', builtin, toml: '' });
  const PERSONAS = [coach('fitness-coach', 'Fitness coach', 'Plans strength, core and mobility sessions around a desk job and a sore lower back', ['fitness'], true), coach('nutrition-coach', 'Nutrition coach with a very long display name for a narrow phone', 'Keeps protein high and takeaway low without counting every calorie', ['fitness'], false), coach('piano-teacher', 'Piano teacher', 'Breaks pieces into bars and builds speed with the metronome', ['music'], true), coach('budget-planner', 'Budget planner', 'Turns a savings target into automatic transfers and a weekly review', ['finance'], true), coach('reading-coach', 'Reading coach', 'Paces a book by chapters and keeps a short summary log', ['learning'], true)];
  const fake = async (method, path, body) => {
    const [p, q] = path.split('?');
    const qs = new URLSearchParams(q ?? '');
    if (method === 'GET' && p === '/v1/runs') return { status: 200, body: RUNS.filter((r) => (!qs.get('goal_id') || r.goal_id === qs.get('goal_id')) && (!qs.get('state') || r.state === qs.get('state'))) };
    const one = p.match(/^\/v1\/runs\/([^/]+)$/);
    if (one && method === 'GET') return VIEWS[one[1]] ? { status: 200, body: VIEWS[one[1]] } : { status: 404, body: { error: 'no run' } };
    if (/^\/v1\/runs\/[^/]+\/(answers|cancel|retry)$/.test(p)) return { status: 202, body: { run_id: p.split('/')[3] } };
    if (p === '/v1/insights') return { status: 200, body: INSIGHTS };
    if (p === '/v1/settings/run-limit') return { status: 200, body: { minutes: method === 'PUT' ? (limit = body.minutes) : limit } };
    if (p === '/v1/keys') return { status: 200, body: [{ setting_id: 'k1', vendor: 'anthropic', last4: '4321', updated_at: ago(600) }] };
    if (p === '/v1/logs') return { status: 202, body: { run_id: null } };
    if (method === 'GET' && p === '/v1/personas') return { status: 200, body: PERSONAS };
    throw 'needs_engine: offline in the gate';
  };
  hooked = async (cmd, args, opts) => {
    called.push(cmd);
    if (cmd === 'app_engine') return fake(args.method, args.path, args.body);
    if (cmd === 'app_account') return { signed_in: true, email: 'owner.with.a.long.address@example.com', user_id: snap.goals[0]?.user_id ?? null };
    if (cmd === 'app_sync') return { pushed: 0, pulled: 0 };
    return real.call(T, cmd, args, opts);
  };
  const { engine } = await import('/src/lib/app/engine.ts');
  engine.call = fake;
  const realFetch = window.fetch.bind(window);
  const ok = (v) => new Response(JSON.stringify(v), { status: 200, headers: { 'Tauri-Response': 'ok', 'Content-Type': 'application/json' } });
  window.fetch = async (input, init) => {
    const url = typeof input === 'string' ? input : input.url;
    const m = /^(?:ipc:\/\/localhost|https?:\/\/ipc\.localhost)\/([^?]+)/.exec(url);
    if (m) {
      const cmd = decodeURIComponent(m[1]);
      called.push(cmd);
      if (cmd === 'app_account' && signedIn) return ok({ signed_in: true, email: 'owner.with.a.long.address@example.com', user_id: snap.goals[0]?.user_id ?? null });
      if (cmd === 'app_sync') return ok({ pushed: 0, pulled: 0 });
      if (cmd === 'app_engine') {
        const a = JSON.parse(init?.body ?? '{}');
        try {
          return ok(await fake(a.method, a.path, a.body));
        } catch (e) {
          return new Response(JSON.stringify(String(e)), { status: 200, headers: { 'Tauri-Response': 'error', 'Content-Type': 'application/json' } });
        }
      }
    }
    return realFetch(input, init);
  };
  };

  const LONG = 'Six pack by Feb 1 with a stronger lower back';
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    const st = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && st.visibility !== 'hidden' && st.display !== 'none';
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
    const w = shell()?.getBoundingClientRect().width ?? innerWidth;
    const wide = [...document.querySelectorAll('main *')].filter((el) => visible(el) && el.getBoundingClientRect().right > w + 1 && !el.closest('.overflow-x-auto'));
    if (wide[0]) hits.push(`past-edge ${wide[0].tagName.toLowerCase()} ${String(wide[0].className).slice(0, 30)}`);
    return hits;
  };
  const textFields = () => [...document.querySelectorAll('main input:not([type=range]):not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]), main textarea')].filter(visible);
  const smallInputs = () =>
    [...document.querySelectorAll('input:not([type=range]):not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]), textarea, select')]
      .filter(visible)
      .filter((el) => parseFloat(getComputedStyle(el).fontSize) < 16)
      .map((el) => `${el.tagName.toLowerCase()}[${el.placeholder || el.id}] ${getComputedStyle(el).fontSize}`);
  const theme = async (t) => {
    await note(`appearance ${t}`);
    await until(`${t} scheme`, () => matchMedia('(prefers-color-scheme: dark)').matches === (t === 'dark'), 15000);
    await sleep(1500);
  };
  const tapText = (t, scope = document) => {
    const b = [...scope.querySelectorAll('button, [role=radio], [role=tab]')].find((x) => x.textContent.trim() === t && visible(x));
    if (!b) throw new Error(`no control ${t}`);
    b.click();
  };
  const calendarView = (v) => async () => {
    tapText(v, document.querySelector('[aria-label="Calendar view"]') ?? document);
    await sleep(1500);
  };
  const newGoal = async (withCategory) => {
    await until('new goal form', () => text().includes('Create goal'), 15000);
    L.type(L.field('Title'), 'Run the Brooklyn half marathon in under two hours without walking on the bridge', false);
    L.type(L.field('Description'), 'I ran a 10k last spring in 58 minutes. I can train four mornings a week before work and one long run on Sunday. My left knee gets sore past 15 km.', false);
    await sleep(1500);
    if (!withCategory) return;
    if (text().includes('Change')) [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Change')?.click();
    await sleep(600);
    tapText('New category');
    await sleep(600);
    const name = document.querySelector('input[placeholder="For example Reading club"]');
    L.type(name, 'Endurance races and long-distance events', false);
    await sleep(400);
    document.querySelector('button[aria-label="Color 3"]')?.click();
    await sleep(600);
  };
  let SCREENS = [];
  const screenList = () => {
  const persona = 'nutrition-coach';
  SCREENS = [
    { name: '01-today', path: '/today', wait: LONG },
    { name: '02-calendar-day', path: '/calendar', prep: calendarView('Day') },
    { name: '03-calendar-week', path: '/calendar', prep: calendarView('Week') },
    { name: '04-calendar-month', path: '/calendar', prep: calendarView('Month') },
    { name: '05-goals', path: '/goals', wait: LONG },
    { name: '06-goal-questions', path: `/goals/${six?.id}`, wait: 'questions to answer', max: 9 },
    { name: '07-goal-plan', path: `/goals/${save?.id}`, wait: 'Plan to approve', max: 5 },
    { name: '08-goal-stopped', path: `/goals/${piano?.id}`, wait: 'Stopped', max: 3 },
    { name: '09-goal-working', path: `/goals/${reading?.id}`, wait: 'Working', max: 3 },
    { name: '10-new-goal', path: '/goals/new', wait: 'Create goal' },
    { name: '11-new-goal-category', path: '/goals/new', prep: () => newGoal(true) },
    { name: '12-inbox', path: '/inbox', wait: 'questions to answer' },
    { name: '12b-dashboard', path: '/dashboard', max: 6 },
    { name: '13-log', path: '/log' },
    { name: '14-settings', path: '/settings', max: 7 },
    { name: '15-settings-profile', path: '/settings/profile' },
    { name: '16-settings-run-limit', path: '/settings', one: true, prep: async () => L.show('Time limit per run') },
    { name: '17-settings-location', path: '/settings/location' },
    { name: '17b-settings-location-on', path: '/settings/location', prep: async () => {
      [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Continue')?.click();
      await sleep(4000);
    } },
    { name: '18-settings-location-log', path: '/settings/location/log' },
    { name: '19-settings-places', path: '/settings/places' },
    { name: '20-coaches', path: '/personas', max: 4 },
    { name: '21-coach', path: `/personas/${encodeURIComponent(persona ?? 'fitness-coach')}`, max: 3 },
    { name: '22-settings-signed-out', path: '/settings', max: 7, before: () => (signedIn = false), after: () => (signedIn = true) }
  ];
  if (smoke) SCREENS = SCREENS.filter((x) => ['06-goal-questions', '14-settings'].includes(x.name));
  if (only.length) SCREENS = SCREENS.filter((x) => only.includes(x.name));
  };
  const open = async (s) => {
    signedIn = true;
    s.before?.();
    await go(s.path === '/inbox' ? '/today' : '/inbox');
    await go(s.path);
    await sleep(1200);
    if (s.wait) await until(`${s.wait} on ${s.name}`, () => text().includes(s.wait), 20000).catch(() => note(`result wait-${s.name} fail no "${s.wait}"`));
    if (s.prep) await s.prep();
    await sleep(600);
  };
  const scrollShots = async (name, max) => {
    const m = main();
    for (let i = 0; i < max; i++) {
      await shot(`${name}${i ? `-${i + 1}` : ''}`);
      if (!m || m.scrollTop + m.clientHeight >= m.scrollHeight - 4) break;
      m.scrollTop += Math.round(m.clientHeight * 0.8);
      await sleep(700);
    }
  };

  const screens = async (t) => {
    await theme(t);
    let bad = 0;
    for (const s of SCREENS) {
      await open(s);
      if (t === 'light') {
        const hits = overflows();
        const small = smallInputs();
        const kept = shell()?.getAttribute('style') ?? '';
        if (shell()) {
          shell().style.right = 'auto';
          shell().style.width = '375px';
        }
        await sleep(900);
        const hits375 = overflows();
        shell()?.setAttribute('style', kept);
        await sleep(400);
        if (hits.length || hits375.length) bad++;
        await result(`overflow-${s.name}`, !hits.length && !hits375.length, `393:${hits[0] ?? 'none'} 375:${hits375[0] ?? 'none'}`);
        await result(`inputs16-${s.name}`, !small.length, small[0] ?? 'all >= 16px');
      }
      if (s.one) await shot(`${t}/${s.name}`);
      else {
        if (main()) main().scrollTop = 0;
        await sleep(400);
        await scrollShots(`${t}/${s.name}`, s.max ?? 3);
      }
      s.after?.();
    }
    return bad;
  };

  const kbField = async (n, name, path, find, prep) => {
    const keep = signedIn;
    await go(path === '/inbox' ? '/today' : '/inbox');
    signedIn = keep;
    await go(path);
    await sleep(2000);
    if (prep) await prep();
    const el = find();
    if (!el) return result(`keyboard-${name}`, false, 'field not found');
    el.scrollIntoView({ block: 'center' });
    await sleep(500);
    el.focus();
    await until('keyboard', () => window.visualViewport.height < innerHeight - 150, 5000).catch(() => {});
    await sleep(900);
    const vv = window.visualViewport;
    const kbTop = vv.offsetTop + vv.height;
    const opened = vv.height < innerHeight - 150;
    const nav = document.querySelector('nav[aria-label="Main"]');
    const nr = nav?.getBoundingClientRect();
    const tabOnKb = !!nav && visible(nav) && nr.top < kbTop - 1 && nr.bottom > vv.offsetTop;
    const r = el.getBoundingClientRect();
    const above = r.bottom <= kbTop + 1 && r.top >= vv.offsetTop - 1;
    const sr = shell()?.getBoundingClientRect();
    const barOk = !sr || sr.top >= vv.offsetTop - 1;
    const wideOk = document.documentElement.scrollWidth <= innerWidth + 1;
    const font = parseFloat(getComputedStyle(el).fontSize);
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const clear = !!hit && (hit === el || el.contains(hit));
    await shot(`keyboard/${String(n).padStart(2, '0')}-${name}`);
    const stayed = document.activeElement === el && window.visualViewport.height < innerHeight - 150;
    const ok = opened && stayed && clear && !tabOnKb && above && barOk && wideOk && font >= 16;
    await result(`keyboard-${name}`, ok, `kb=${opened ? Math.round(innerHeight - vv.height) : 'none'} stayed=${stayed} clear=${clear ? 'yes' : hit?.tagName} tab=${tabOnKb ? 'on' : 'off'} f=${Math.round(r.top)}-${Math.round(r.bottom)}/${Math.round(kbTop)} ${font}px`);
    el.blur();
    await sleep(900);
  };
  const byPlaceholder = (p) => () => document.querySelector(`[placeholder="${p}"]`);
  const keyboard = async () => {
    if (smoke) return kbField(1, 'goal-chat', `/goals/${six?.id}`, byPlaceholder('Message your personas'));
    {
    await kbField(1, 'goal-chat', `/goals/${six?.id}`, byPlaceholder('Message your personas'));
    await kbField(2, 'new-goal-title', '/goals/new', () => L.field('Title'));
    await kbField(3, 'new-goal-description', '/goals/new', () => L.field('Description'));
    await kbField(4, 'number-question', `/goals/${six?.id}`, () => [...document.querySelectorAll('main input[inputmode="decimal"]')].find(visible));
    await kbField(5, 'log-quick-log', '/log', byPlaceholder('Log a meal, weight or note'));
    await kbField(6, 'today-quick-log', '/today', byPlaceholder('Log a meal, weight or note'));
    await kbField(7, 'new-category-name', '/goals/new', byPlaceholder('For example Reading club'), () => newGoal(true));
    }
    await kbField(8, 'attach-link', '/goals/new', () => document.querySelector('input[aria-label="Link"]'), async () => {
      [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Link')?.click();
      await sleep(800);
    });
    await kbField(9, 'places-search', '/settings/places', byPlaceholder('Search a place or address'));
    await kbField(10, 'coach-rule', SCREENS.find((x) => x.name === '21-coach')?.path ?? '/personas', byPlaceholder('Add a rule'));
    const nameOf = (el) => (document.querySelector(`label[for="${el.id}"]`)?.textContent ?? el.getAttribute('aria-label') ?? el.placeholder ?? 'field').trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 30);
    let n = 11;
    await go('/inbox');
    await go(`/goals/${six?.id}`);
    await sleep(2500);
    const answers = () => [...document.querySelectorAll('main input[inputmode="decimal"], main input[placeholder="Type your answer"], main textarea[placeholder="Type your answer"]')].filter(visible);
    const qs = answers().length;
    await note(`question inputs ${qs}`);
    for (let i = 0; i < qs; i++) await kbField(n++, `question-${i + 1}`, `/goals/${six?.id}`, () => answers()[i]);
    for (const out of [false, true]) {
      signedIn = !out;
      await go('/inbox');
      await go('/settings');
      await sleep(2500);
      const names = textFields().map(nameOf);
      await note(`settings ${out ? 'signed-out' : 'signed-in'} text fields ${names.length}`);
      for (const [i, name] of names.entries()) await kbField(n++, `settings-${out ? 'out-' : ''}${name}`, '/settings', () => textFields()[i]);
    }
    signedIn = true;
  };

  const NATIVE = /^(Speak a message|Speak a log entry|Stop speaking|Photo|File|Log with a photo|Open Settings)$/;
  const labelOf = (b) => (b.getAttribute('aria-label') || b.innerText || b.title || '').trim().replace(/\s+/g, ' ').slice(0, 50);
  const keyOf = (b) => `${b.tagName}:${labelOf(b).replace(/\d+/g, '#')}`;
  const tappables = () => [...document.querySelectorAll('button, a[href], [role=button], [role=radio], [role=tab], [role=switch], [role=checkbox]')].filter((b) => visible(b) && !b.disabled && b.getAttribute('aria-disabled') !== 'true');
  const hash = (s) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7);
  const sig = () =>
    [
      location.pathname + location.search,
      hash(document.body.innerText),
      document.querySelectorAll('*').length,
      [...document.querySelectorAll('[aria-pressed],[aria-checked],[aria-expanded],[aria-selected],[data-state]')].map((e) => `${e.getAttribute('aria-pressed')}${e.getAttribute('aria-checked')}${e.getAttribute('aria-expanded')}${e.getAttribute('aria-selected')}${e.getAttribute('data-state')}`).join(''),
      [...document.querySelectorAll('input, textarea')].map((e) => e.value).join('|').length
    ].join('~');
  const seenGlobal = new Set();
  const buttons = async () => {
    let dead = 0;
    let tapped = 0;
    for (const s of SCREENS) {
      await open(s);
      const keys = [];
      for (const b of tappables()) {
        const k = keyOf(b);
        const inNav = !!b.closest('nav[aria-label="Main"]') || !!b.closest('header');
        if (keys.includes(k) || (inNav && seenGlobal.has(k))) continue;
        if (inNav) seenGlobal.add(k);
        keys.push(k);
      }
      await note(`tap ${s.name} ${keys.length} controls`);
      const cap = smoke ? 8 : 120;
      if (keys.length > cap) await note(`tap ${s.name} truncated ${keys.length - cap} not tapped`);
      for (const k of keys.slice(0, cap)) {
        const label = k.split(':').slice(1).join(':');
        if (NATIVE.test(label)) {
          await note(`tap ${s.name} phone-only ${label}`);
          continue;
        }
        await open(s);
        const b = tappables().find((x) => keyOf(x) === k);
        if (!b) {
          await note(`tap ${s.name} gone ${label}`);
          continue;
        }
        const href = b.getAttribute('href') ?? '';
        if (/^https?:/.test(href)) {
          await note(`tap ${s.name} external-link ${label}`);
          continue;
        }
        b.scrollIntoView({ block: 'center' });
        await sleep(400);
        const before = sig();
        await sleep(700);
        const still = sig();
        called.length = 0;
        await note(`tap-begin ${s.name} ${label}`);
        if (typeof b.click === "function") b.click(); else b.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        await sleep(2600);
        const after = sig();
        const cmds = [...new Set(called.filter((c) => !['app_dev_mark', 'app_snapshot', 'app_engine'].includes(c)))];
        tapped++;
        const what = after !== still ? 'ok' : cmds.length ? `native:${cmds.join(',')}` : before !== still ? 'noisy' : 'nothing';
        if (what === 'nothing') dead++;
        await note(`tap ${s.name} ${what} ${label}`);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(300);
      }
    }
    await note(`taps ${tapped} tapped, ${dead} did nothing`);
  };

  try {
    await until('app rendered', () => text().includes('Skip for now') || (text().includes('Calendar') && text().includes('Dashboard')), 120000);
    await skipSetup();
    await setup();
    screenList();
    const { data } = await import('/src/lib/app/data.svelte.ts');
    await data.load();
    const acct = await T.invoke('app_account', {}).catch((e) => ({ email: `error ${e}` }));
    await result('harness-hook', String(acct?.email).startsWith('owner.with'), `account ${acct?.email} via ipc fetch`);
    const lightBad = await screens('light');
    await result('overflow-all', lightBad === 0, `${lightBad} screens with overflow`);
    if (!smoke) await screens('dark');
    await theme('light');
    if (only.length) {
      await go('/inbox');
      await sleep(2500);
      const link = document.querySelector('a[href$="/chat"]');
      const href = link?.getAttribute('href');
      link?.click();
      await sleep(2500);
      await note(`probe inbox-link href=${href} now=${location.pathname}`);
      await result('inbox-waiting-link', !!href && location.pathname === href.replace('/chat', ''), `${href} -> ${location.pathname}`);
    }
    if (!window.AROH_E2E.nokb) await keyboard();
    if (!window.AROH_E2E.notaps) await buttons();
    await note('done');
  } catch (e) {
    await fail(e);
  }
})();
