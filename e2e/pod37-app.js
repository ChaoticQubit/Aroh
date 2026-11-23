(async () => {
  const L = window.AROH_LIB;
  const { invoke, sleep, mark: note, until, go, started, button } = L;
  const theme = window.AROH_E2E.theme ?? 'light';
  const SENTENCE = 'Two walks a day, core work at home after 6';
  const results = [];
  const result = async (name, ok, detail = '') => {
    results.push(ok);
    await note(`result ${name} ${ok ? 'pass' : 'fail'} ${String(detail)}`.replace(/[^ -~]/g, '?').slice(0, 80));
  };
  const shot = async (name) => {
    await sleep(900);
    await note(`shot ${theme}/app-${name}`);
    await sleep(2500);
  };
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
  };
  const overflow = () => {
    const hits = [];
    if (document.documentElement.scrollWidth > innerWidth + 1) hits.push(`html ${document.documentElement.scrollWidth}`);
    const wide = [...document.querySelectorAll('main *')].filter((el) => visible(el) && el.getBoundingClientRect().right > innerWidth + 1);
    if (wide[0]) hits.push(`past-edge ${wide[0].tagName.toLowerCase()} ${String(wide[0].className).slice(0, 24)}`);
    return hits;
  };
  const checkOverflow = async (name) => {
    const o = overflow();
    await result(`overflow-${name}`, o.length === 0, o[0] ?? '');
  };
  const byLabel = (text) => {
    const l = [...document.querySelectorAll('main label')].find((x) => x.textContent.trim() === text);
    return l && (l.htmlFor ? document.getElementById(l.htmlFor) : l.querySelector('input, textarea'));
  };
  const kbField = async (name, el) => {
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
    const sr = document.querySelector('.fixed.inset-x-0.top-0')?.getBoundingClientRect();
    const barOk = !sr || sr.top >= vv.offsetTop - 1;
    const wideOk = document.documentElement.scrollWidth <= innerWidth + 1;
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const clear = !!hit && (hit === el || el.contains(hit));
    const font = parseFloat(getComputedStyle(el).fontSize);
    await shot(`kb-${name}`);
    const stayed = document.activeElement === el && window.visualViewport.height < innerHeight - 150;
    const ok = opened && stayed && clear && !tabOnKb && above && barOk && wideOk && font >= 16;
    await result(`keyboard-${name}`, ok, `kb=${opened} stayed=${stayed} clear=${clear} tab=${tabOnKb} above=${above} bar=${barOk} wide=${wideOk} ${font}px`);
  };
  const spaces = async (name, el, sample) => {
    const { value, forced } = await L.keys(el, sample);
    await result(`spaces-${name}`, value === sample && !forced, `forced=${forced} got "${value.slice(0, 40)}"`);
    el.blur();
    await sleep(700);
  };
  const pick = (start) => {
    const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim().startsWith(start));
    if (!b) throw new Error(`no button ${start}`);
    return b;
  };
  const viaSheet = async (name, path) => {
    await go(path);
    await sleep(1500);
    const edit = [...document.querySelectorAll('main button')].find((b) => /^Edit /.test(b.getAttribute('aria-label') ?? b.textContent.trim()));
    edit?.click();
    await sleep(900);
    const open = [...document.querySelectorAll('a')].find((a) => a.textContent.trim() === 'Open session card');
    open?.click();
    await until(`card page from ${name}`, () => location.pathname.startsWith('/actions/'), 8000).catch(() => {});
    await result(`open-from-${name}`, location.pathname.startsWith('/actions/'), `${edit ? 'sheet' : 'no row'} ${location.pathname}`);
  };
  const shows = (t) => document.body.innerText.includes(t) || [...document.querySelectorAll('main input, main textarea')].some((el) => el.value.includes(t));
  const page = (id) => invoke('app_card_page', { id });
  const workout = (p) => p.action.card.blocks.find((b) => b.kind === 'workout');

  try {
    await started();
    await note(`seeded ${await invoke('app_dev_seed')}`);
    const { data } = await import('/src/lib/app/data.svelte.ts');
    await data.load();
    const now = Date.now();
    const strength = data.snapshot.actions
      .filter((a) => a.title === 'Strength session' && a.status === 'planned' && Date.parse(a.start_at) > now)
      .sort((a, b) => a.start_at.localeCompare(b.start_at));
    const meal = data.snapshot.actions.find((a) => a.title === 'Meal prep' && Date.parse(a.start_at) > now);
    const six = data.snapshot.goals.find((g) => g.title === 'Six pack by Feb 1');
    await result('sample-cards', !!strength[0]?.card && !!meal?.card, `strength ${strength.length}`);

    await go(`/goals/${six.id}`);
    await sleep(1500);
    const link = [...document.querySelectorAll('main a[href^="/actions/"]')].find(visible);
    link?.click();
    await until('card page from the goal list', () => location.pathname.startsWith('/actions/'), 8000).catch(() => {});
    await result('open-from-goal', location.pathname.startsWith('/actions/'), location.pathname);

    await viaSheet('today', '/today');
    await viaSheet('calendar', '/calendar');
    await go(`/actions/${strength[0].id}`);
    await until('card', () => shows('Back squat'), 10000);
    await checkOverflow('card');
    await shot('01-card');
    document.querySelector('main').scrollTop = 600;
    await shot('01-card-scrolled');
    document.querySelector('main').scrollTop = 0;

    const why = byLabel('Why');
    await kbField('card-why', why);
    await spaces('card-why', why, SENTENCE);
    await until('scope sheet', () => document.body.innerText.includes('Change which sessions?'), 4000).catch(() => {});
    await result('scope-asked-first-edit', document.body.innerText.includes('Change which sessions?'));
    await checkOverflow('scope');
    await shot('02-scope');
    pick('This session only').click();
    await sleep(1500);
    let p = await page(strength[0].id);
    await result('why-saved-this-session', p.action.card.why === SENTENCE, p.action.card.why.slice(0, 30));
    const next = await page(strength[1].id);
    await result('next-session-unchanged', next.action.card.why !== SENTENCE);
    await result('undo-shown', document.body.innerText.includes('Saved for this session'));
    await shot('03-saved-undo');

    const name = document.querySelector('main input[id$="-name-0"]');
    await kbField('exercise-name', name);
    await spaces('exercise-name', name, 'Front squat with a pause at the bottom');
    const rest = [...document.querySelectorAll('main input[inputmode="decimal"]')].filter(visible)[3];
    await kbField('rest-number', rest);
    await spaces('rest-number', rest, '45');
    const notes = byLabel('Notes');
    await kbField('notes', notes);
    await spaces('notes', notes, SENTENCE);
    await sleep(1200);
    p = await page(strength[0].id);
    const w = workout(p);
    await result('cells-saved', w.exercises[0].name === 'Front squat with a pause at the bottom' && w.exercises[0].rest_s === 45 && p.action.notes === SENTENCE, `${w.exercises[0].rest_s}`);

    const rows = () => document.querySelectorAll('main ol > li[data-row]').length;
    const before = rows();
    button('Add exercise').click();
    await sleep(600);
    await result('add-row', rows() === before + 1, `${before}->${rows()}`);
    const last = [...document.querySelectorAll('main li[data-row]')].filter((li) => li.querySelector('input[id*="-name-"]')).at(-1);
    const r = last.getBoundingClientRect();
    const pe = (type, x) => last.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: r.top + 8, pointerId: 7 }));
    pe('pointerdown', r.right - 20);
    pe('pointermove', r.right - 60);
    pe('pointermove', r.right - 160);
    pe('pointerup', r.right - 160);
    await sleep(600);
    await result('swipe-delete', rows() === before, `${rows()}`);
    const firstGrip = document.querySelector('main [data-grip]');
    firstGrip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    await sleep(1200);
    p = await page(strength[0].id);
    await result('reorder', workout(p).exercises[1].name === 'Front squat with a pause at the bottom', workout(p).exercises[0].name);
    await checkOverflow('editing');
    await shot('04-edited');
    button('Undo').click();
    await sleep(1500);
    p = await page(strength[0].id);
    await result('undo-restores', p.action.card.why === strength[0].card.why && workout(p).exercises[0].name === 'Back squat');

    button('Start session').click();
    await sleep(800);
    button('Set 1 not done, tick to log as planned').click();
    await sleep(500);
    [...document.querySelectorAll('main button')].find((b) => /^Set 2 reps/.test(b.getAttribute('aria-label') ?? ''))?.click();
    await sleep(400);
    button('One rep less').click();
    await sleep(500);
    const composer = document.querySelector('textarea[placeholder="Say \'last set 7 reps\'"]');
    await kbField('session-composer', composer);
    await spaces('session-composer', composer, 'last set 7 reps');
    button('Send').click();
    await sleep(800);
    p = await page(strength[0].id);
    const sets = p.log?.sets ?? [];
    await result('log-tap-step-voice', sets.length === 2 && sets.some((s) => s.set === 0 && s.reps === 8) && sets.some((s) => s.set === 1 && s.reps === 7), JSON.stringify(sets.map((s) => [s.set, s.reps])));
    await checkOverflow('logging');
    await shot('05-logging');
    button('Hard').click();
    await sleep(500);
    p = await page(strength[0].id);
    await result('effort-saved', p.log?.effort === 'hard', p.log?.effort);

    await go(`/actions/${strength[1].id}/chat?text=${encodeURIComponent('swap lunges for split squats, 45 min')}`);
    await until('chat scope', () => document.body.innerText.includes('Change which sessions?'), 8000).catch(() => {});
    await result('chat-scope-first', document.body.innerText.includes('Change which sessions?'));
    await checkOverflow('chat-scope');
    await shot('06-chat-scope');
    pick('This and all future sessions').click();
    await until('coach answer', () => document.querySelectorAll('main [aria-label="Card chat"] li').length >= 2, 15000).catch(() => {});
    await result('chat-offline-answer', document.body.innerText.includes('engine'), document.querySelector('main [aria-label="Card chat"]')?.innerText.slice(-60));
    await shot('07-chat-offline');

    await go(`/actions/${meal.id}`);
    await until('meal card', () => shows('Chicken rice bowls'), 8000);
    await checkOverflow('meal');
    await shot('08-meal');
    button('Start session').click();
    await sleep(500);
    button('Finish session').click();
    await sleep(1500);
    const done = (await page(meal.id)).action.status;
    await result('finish-marks-done', done === 'done', done);
    await note(`summary ${results.filter(Boolean).length}/${results.length}`);
    await note('done');
  } catch (e) {
    await note(`shot 99-failure ${String(e).replace(/[^ -~]/g, '?').slice(0, 60)}`);
  }
})();
