(async () => {
  const { cfg, invoke, sleep, mark, text, until, go, button, show, started, fail } = window.AROH_LIB;
  const DAY = 864e5;
  const HOURS = { 'Strength session': 18, 'Core session': 7, 'Meal prep': 10, 'Piano practice': 20, 'Weekly budget review': 12 };
  const LOCKED = 'Piano practice';
  const check = (ok, what) => {
    if (!ok) throw new Error(what);
  };
  const part = (iso, zone, opts) => new Intl.DateTimeFormat('en-GB', { ...opts, timeZone: zone }).format(new Date(iso));
  const ymd = (iso, zone) => new Intl.DateTimeFormat('en-CA', { year: 'numeric', month: '2-digit', day: '2-digit', timeZone: zone }).format(new Date(iso));
  const hour = (iso, zone) => Number(part(iso, zone, { hour: '2-digit', hourCycle: 'h23' }));
  const snap = () => invoke('app_snapshot', { from: new Date(Date.now() - DAY).toISOString(), to: new Date(Date.now() + 10 * DAY).toISOString() });
  const onDay = (s, day) => s.actions.filter((a) => a.series_id && a.status === 'planned' && ymd(a.start_at, s.zone) === day);
  const openDay = async (day, zone) => {
    const noon = `${day}T12:00:00Z`;
    const name = part(noon, 'UTC', { weekday: 'short' });
    const date = String(Number(day.slice(8)));
    const find = () => [...document.querySelectorAll('[role=radio]')].find((b) => b.textContent.includes(name) && b.textContent.includes(date));
    for (let i = 0; i < 2 && !find(); i++) {
      button('Next').click();
      await sleep(1000);
    }
    check(find(), `no week strip day ${day} in ${zone}`);
    find().click();
    await sleep(1500);
  };
  const shot = async (name) => {
    await mark(`shot ${name}`);
    await sleep(3000);
  };
  const shoot = async (prefix, s, day) => {
    await go('/calendar');
    await openDay(day, s.zone);
    show(LOCKED);
    await sleep(1000);
    await shot(`${prefix}-calendar`);
    await go('/today');
    await shot(`${prefix}-today`);
  };
  const flexibleKeepWallTime = (s, day) => {
    const rows = onDay(s, day).filter((a) => !a.locked);
    check(rows.length > 0, `no flexible series row on ${day}`);
    for (const a of rows) check(hour(a.start_at, s.zone) === HOURS[a.title], `${a.title} at ${part(a.start_at, s.zone, { hour: '2-digit', minute: '2-digit' })} in ${s.zone}`);
  };
  const lockedKept = (s) => {
    const a = s.actions.find((x) => x.title === LOCKED && x.locked);
    check(a && a.start_at === cfg.locked, `locked ${LOCKED} moved: ${a && a.start_at} vs ${cfg.locked}`);
  };

  try {
    await started();
    if (cfg.phase === 'home') {
      check(await invoke('app_dev_seed'), 'sample not loaded');
      await sleep(2000);
      let s = await snap();
      check(s.zone === 'Europe/Berlin', `zone ${s.zone}`);
      const day = ymd(new Date(Date.now() + DAY).toISOString(), s.zone);
      const piano = onDay(s, day).find((a) => a.title === LOCKED);
      check(piano, `no ${LOCKED} on ${day}`);
      await invoke('app_edit_action', { id: piano.id, startAt: piano.start_at, endAt: piano.end_at, delete: false });
      await sleep(1500);
      s = await snap();
      flexibleKeepWallTime(s, day);
      await mark(`day ${day}`);
      await mark(`locked ${piano.start_at}`);
      await shoot('01-berlin', s, day);
    } else {
      await until('zone note', () => text().includes('Inbox'), 30000);
      const s = await snap();
      check(s.zone === cfg.zone, `zone ${s.zone}, expected ${cfg.zone}`);
      check(s.notes.length === cfg.notes && s.notes[0].title === 'Time zone changed', `notes ${s.notes.length}`);
      check(s.notes[0].body.includes(`Now in ${cfg.zone}`), s.notes[0].body.slice(0, 60));
      lockedKept(s);
      flexibleKeepWallTime(s, cfg.day);
      await mark(`fixed ${LOCKED} at ${part(cfg.locked, s.zone, { hour: '2-digit', minute: '2-digit' })} ${s.zone}`);
      await shoot(cfg.prefix, s, cfg.day);
      await go('/inbox');
      await until('note card', () => text().includes('Time zone changed'), 10000);
      show('Time zone changed');
      await sleep(1000);
      await shot(`${cfg.prefix}-inbox`);
    }
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
