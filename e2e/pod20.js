(async () => {
  const { cfg, invoke, sleep, mark, text, until, go, button, show, say, started, signUp, saveKey, fail } = window.AROH_LIB;
  const DAY = 864e5;
  const ymd = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  const dayOf = (offset) => ymd(new Date(Date.now() + offset * DAY));
  const localDay = (iso) => ymd(new Date(iso));
  const hhmm = (iso) => new Date(iso).toTimeString().slice(0, 5);
  const snap = () => invoke('app_snapshot', { from: new Date(Date.now() - DAY).toISOString(), to: new Date(Date.now() + 20 * DAY).toISOString() });
  const live = (s, title) => s.actions.filter((a) => a.title === title && !a.deleted && a.status === 'planned').sort((a, b) => a.start_at.localeCompare(b.start_at));
  const block = (s, title) => s.busy.find((b) => b.title === title && b.all_day);
  const card = (needle) => [...document.querySelectorAll('[data-slot=card]')].find((c) => c.textContent.includes(needle));
  const press = (root, starts) => {
    const b = [...root.querySelectorAll('button')].find((x) => x.textContent.trim().startsWith(starts));
    if (!b) throw new Error(`no button ${starts}`);
    b.click();
  };
  const check = (ok, what) => {
    if (!ok) throw new Error(what);
  };
  const openDay = async (offset) => {
    const target = new Date(Date.now() + offset * DAY);
    const name = target.toLocaleDateString('en-GB', { weekday: 'short' });
    const find = () => [...document.querySelectorAll('[role=radio]')].find((b) => b.textContent.includes(name) && b.textContent.includes(String(target.getDate())));
    for (let i = 0; i < 2 && !find(); i++) {
      button('Next').click();
      await sleep(1000);
    }
    const day = find();
    check(day, `no week strip day ${ymd(target)}`);
    day.click();
    await sleep(1200);
  };
  const sheet = () => document.querySelector('[data-slot=sheet-content]');
  const edit = async (title) => {
    const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === `Edit ${title}`);
    check(b, `no Edit ${title}`);
    b.click();
    await until('edit sheet', () => sheet() && sheet().textContent.includes(title), 10000);
    await sleep(800);
  };
  const fixedSwitch = () => sheet()?.querySelector('[role=switch]');

  try {
    await started();
    await signUp();
    await saveKey();
    await go('/today');
    await go('/goals/new');
    await say('Six pack by Feb 1');
    await until('question cards', () => text().includes('How many days'), 120000);
    for (const answer of ['4', 'No nuts']) await say(answer);
    await until('plan', () => text().includes('Approve plan'), 120000);
    const dentist = document.querySelector('[aria-label^="Fixed time for Dentist"]');
    check(dentist, 'no Fixed time switch on the Dentist item');
    check(!document.querySelector('[aria-label^="Fixed time for Morning plank"]'), 'a repeating item has a Fixed time switch');
    dentist.click();
    await sleep(600);
    check(dentist.getAttribute('aria-checked') === 'true', 'Dentist switch did not turn on');
    dentist.scrollIntoView({ block: 'center' });
    await mark('shot 01-plan-fixed-time');
    button('Approve plan').click();
    await until('plan written', () => text().split('The plan is on your calendar').length > 2, 120000);
    let fixed = null;
    for (let i = 0; i < 20 && !fixed; i++) {
      await sleep(1500);
      fixed = live(await snap(), 'Dentist').find((a) => a.locked) ?? null;
    }
    check(fixed && localDay(fixed.start_at) === dayOf(1) && hhmm(fixed.start_at) === '08:00', 'approved Dentist is not fixed');
    const planned = await snap();
    check(!live(planned, 'Core workout')[0].locked, 'core workout got fixed');
    await mark('shot 02-approved');

    await invoke('plugin:aroh-calendar|seed', { events: cfg.seed });
    await go('/settings');
    const label = document.querySelector('label[for$="-cal"]');
    document.getElementById(label.htmlFor).click();
    await sleep(1000);

    let found = false;
    for (let i = 0; i < 40 && !found; i++) {
      await go(i % 2 ? '/today' : '/inbox');
      await sleep(4000);
      found = location.pathname === '/inbox' && text().includes('Work meetings');
    }
    check(found, 'no triage card');
    check(!card('Work meetings').textContent.includes('Conference'), 'the all-day Conference is in a busy-time group');
    show('Work meetings');
    await mark('shot 03-triage-timed-only');
    press(card('Work meetings'), 'Keep as busy time');
    await sleep(500);
    press(card('Work meetings'), 'Send answer');
    let kept = false;
    for (let i = 0; i < 30 && !kept; i++) {
      await sleep(3000);
      await invoke('app_sync').catch(() => {});
      kept = (await snap()).busy.some((b) => b.title === 'Team standup' && !b.all_day);
    }
    check(kept, 'standup not kept as busy time');

    await go('/calendar');
    await openDay(1);
    await until('Conference card', () => card('Conference') && card('Conference').textContent.includes('What does it do to the day?'), 60000);
    const suggested = [...card('Conference').querySelectorAll('button[aria-pressed]')].find((b) => b.textContent.includes('Suggested'));
    check(suggested && suggested.textContent.trim().startsWith('Busy day'), 'Busy day is not the kept suggestion');
    card('Conference').scrollIntoView({ block: 'center' });
    await mark('shot 04-kind-suggested');
    press(card('Conference'), 'Busy day');
    await sleep(3000);

    const busy = await snap();
    check(block(busy, 'Conference').day_kind === 'busy', 'Conference is not busy');
    const workout = live(busy, 'Core workout')[0];
    check(localDay(workout.start_at) === dayOf(2) && hhmm(workout.start_at) === '18:00', `core workout not moved: ${workout.start_at}`);
    const kept = live(busy, 'Dentist')[0];
    check(kept.id === fixed.id && kept.start_at === fixed.start_at, 'the fixed Dentist moved');
    check(live(busy, 'Morning plank').some((a) => localDay(a.start_at) === dayOf(1)), 'the daily plank left the busy day');
    await openDay(1);
    await until('busy caption', () => text().includes('Conference is a busy day.'), 15000);
    const caption = [...document.querySelectorAll('p')].find((p) => p.textContent.trim() === 'Conference is a busy day.');
    const box = caption.getBoundingClientRect();
    const covered = [...document.querySelectorAll('[role=list] *')].filter((el) => ['Morning plank', 'Dentist'].includes(el.textContent.trim())).some((el) => {
      const r = el.getBoundingClientRect();
      return r.top < box.bottom && box.top < r.bottom && r.left < box.right && box.left < r.right;
    });
    check(!covered, 'the busy-day caption covers an action block');
    show('Dentist');
    await mark('shot 05-busy-day-fixed-stays');

    await go('/inbox');
    await until('stuck card', () => text().includes('No free day for'), 30000);
    show('No free day for');
    await mark('shot 06-inbox-stuck');
    await until('clash card', () => text().includes('Fixed action clashes: Dentist'), 30000);
    show('Fixed action clashes: Dentist');
    await mark('shot 07-inbox-fixed-clash');

    await go('/calendar');
    await openDay(2);
    await edit('Core workout');
    const sw = fixedSwitch();
    check(sw && sw.getAttribute('aria-checked') === 'false', 'no off Fixed time switch on the edit sheet');
    await mark('shot 08-edit-fixed-off');
    sw.click();
    await sleep(2500);
    check(live(await snap(), 'Core workout')[0].locked, 'switch did not fix the workout');
    await mark('shot 09-edit-fixed-on');
    fixedSwitch().click();
    await sleep(2500);
    check(!live(await snap(), 'Core workout')[0].locked, 'switch did not free the workout');
    button('Cancel').click();
    await sleep(800);
    await invoke('app_sync');
    await sleep(3000);

    const goal = (await snap()).goals[0];
    await go(`/goals/${goal.id}`);
    show('Coaches on this goal');
    await mark('shot 10-goal-coaches');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
