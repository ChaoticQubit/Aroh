(async () => {
  const { cfg, invoke, sleep, mark, text, until, go, button, show, started, signUp, saveKey, planGoal, fail } = window.AROH_LIB;
  const DAY = 864e5;
  const MIN = 6e4;
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
  const choose = async (title, kind) => {
    await until(`kind card for ${title}`, () => card(title) && card(title).textContent.includes('What does it do to the day?'), 30000);
    const c = card(title);
    c.scrollIntoView({ block: 'center' });
    press(c, kind);
    await sleep(3000);
  };
  const sheet = () => document.querySelector('[data-slot=sheet-content]');
  const edit = async (title) => {
    const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === `Edit ${title}`);
    check(b, `no Edit ${title}`);
    b.click();
    await until('edit sheet', () => sheet() && sheet().textContent.includes(title), 10000);
    await sleep(800);
  };
  const inSheet = (label) => {
    const b = [...sheet().querySelectorAll('button')].find((x) => x.textContent.trim() === label || x.getAttribute('aria-label') === label);
    check(b, `no sheet button ${label}`);
    b.click();
  };

  try {
    await started();
    await signUp();
    await saveKey();
    await mark('shot 01-signed-in');
    await planGoal({ plan: 'shot 02-plan' });
    const planned = await snap();
    const workout = live(planned, 'Core workout')[0];
    const plank = live(planned, 'Morning plank')[0];
    check(workout && localDay(workout.start_at) === dayOf(1), 'core workout is not tomorrow');
    check(plank && localDay(plank.start_at) === dayOf(1), 'first plank is not tomorrow');

    await invoke('plugin:aroh-calendar|seed', { events: cfg.seed });
    await go('/settings');
    const label = document.querySelector('label[for$="-cal"]');
    document.getElementById(label.htmlFor).click();
    show('Read device calendars');
    await mark('shot 03-calendars-on');

    let found = false;
    for (let i = 0; i < 40 && !found; i++) {
      await go(i % 2 ? '/today' : '/inbox');
      await sleep(4000);
      found = location.pathname === '/inbox' && text().includes('Work meetings:');
    }
    check(found, 'no triage cards');
    await mark('shot 04-triage-cards');
    const imported = await snap();
    for (const t of ['Conference', "Mom's birthday", 'Vacation']) check(block(imported, t) && block(imported, t).day_kind === null, `${t} not imported without a kind`);

    await go('/calendar');
    await openDay(1);
    await until('Conference chip', () => text().includes('Conference'), 30000);
    const first = card('Conference');
    check(first && first.textContent.includes('Suggested'), 'no suggestion on the Conference card');
    const suggested = [...first.querySelectorAll('button[aria-pressed]')].find((b) => b.textContent.includes('Suggested'));
    check(suggested && suggested.textContent.trim().startsWith('Busy day'), 'Busy day is not the suggestion');
    show('What does it do to the day?');
    await mark('shot 05-kind-suggested');
    await choose('Conference', 'Busy day');

    const busy = await snap();
    check(block(busy, 'Conference').day_kind === 'busy', 'Conference is not busy');
    const moved = live(busy, 'Core workout')[0];
    check(moved && localDay(moved.start_at) === dayOf(2) && hhmm(moved.start_at) === hhmm(workout.start_at), `core workout not moved to the next day: ${moved && moved.start_at}`);
    const plankMoved = live(busy, 'Morning plank').find((a) => localDay(a.start_at) === dayOf(2));
    check(plankMoved && !live(busy, 'Morning plank').some((a) => localDay(a.start_at) === dayOf(1)), 'plank not moved off the busy day');
    await openDay(1);
    await until('busy caption', () => text().includes('Conference is a busy day.'), 15000);
    await mark('shot 06-busy-day');

    await choose("Mom's birthday", 'Reminder');
    await choose('Vacation', 'Away');
    const kinds = await snap();
    check(block(kinds, "Mom's birthday").day_kind === 'reminder' && block(kinds, 'Vacation').day_kind === 'away', 'kinds not stored');
    check(live(kinds, 'Evening stretch').length === 1 && localDay(live(kinds, 'Evening stretch')[0].start_at) === dayOf(3), 'away moved an action');

    await go('/today');
    await go('/calendar');
    await openDay(2);
    await until('moved workout on the reminder day', () => text().includes('Core workout') && text().includes("Mom's birthday"), 15000);
    show('Core workout');
    await mark('shot 07-reminder-day-with-moved-actions');

    const chip = [...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') ?? '').startsWith("Mom's birthday, Reminder"));
    check(chip, 'no Reminder chip button');
    chip.click();
    await until('reopened card', () => card("Mom's birthday") && card("Mom's birthday").textContent.includes('Close'), 10000);
    const pressed = card("Mom's birthday").querySelector('button[aria-pressed=true]');
    check(pressed && pressed.textContent.trim().startsWith('Reminder'), 'current kind not marked');
    await mark('shot 08-chip-reopened');
    press(card("Mom's birthday"), 'Close');
    await sleep(800);

    await edit('Core workout');
    await mark('shot 09-edit-sheet');
    inSheet('Start later or longer by 15 min');
    inSheet('Start later or longer by 15 min');
    await sleep(300);
    inSheet('Save');
    await sleep(2000);
    await invoke('app_sync');
    await sleep(3000);
    const shifted = live(await snap(), 'Core workout')[0];
    check(Date.parse(shifted.start_at) - Date.parse(moved.start_at) === 30 * MIN && shifted.locked, `move not saved: ${shifted.start_at}`);

    await edit('Core workout');
    inSheet('Duration later or longer by 15 min');
    await sleep(300);
    inSheet('Save');
    await sleep(2000);
    await invoke('app_sync');
    await sleep(3000);
    const longer = live(await snap(), 'Core workout')[0];
    check(Date.parse(longer.end_at) - Date.parse(longer.start_at) === 45 * MIN, `resize not saved: ${longer.end_at}`);
    show('Core workout');
    await mark('shot 10-moved-and-resized');

    await edit('Morning plank');
    inSheet('Delete');
    await until('delete confirm', () => sheet().textContent.includes('It leaves your calendar on every device.'), 5000);
    await mark('shot 11-delete-confirm');
    const confirm = [...sheet().querySelectorAll('[role=alertdialog] button')].find((b) => b.textContent.trim() === 'Delete');
    check(confirm, 'no Delete in the confirm');
    confirm.click();
    await sleep(3000);
    check(!live(await snap(), 'Morning plank').some((a) => a.id === plankMoved.id), 'plank not deleted');
    await invoke('app_sync');

    let adapted = false;
    for (let i = 0; i < 30 && !adapted; i++) {
      await invoke('app_sync');
      await go('/today');
      await go('/inbox');
      adapted = text().includes('Hotel-room stretch') || text().includes('hotel-room stretch');
      if (!adapted) await sleep(8000);
    }
    check(adapted, 'no Away proposal in the Inbox');
    show('hotel-room');
    await mark('shot 12-away-proposal');

    await go('/calendar');
    button('Month').click();
    await sleep(1500);
    show('Vacation');
    await mark('shot 13-month-all-day-bars');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
