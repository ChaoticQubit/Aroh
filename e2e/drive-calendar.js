(async () => {
  const { cfg, invoke, sleep, mark, text, until, go, button, show, openTomorrow, say, started, signUp, saveKey, fail } = window.AROH_LIB;
  const card = (needle) => [...document.querySelectorAll('[data-slot=card]')].find((c) => c.textContent.includes(needle));
  const press = (root, name) => {
    const b = [...root.querySelectorAll('button')].find((x) => x.textContent.trim() === name);
    if (!b) throw new Error(`no button ${name}`);
    b.click();
  };
  const decide = async (group, ignore) => {
    const c = card(`${group}:`);
    if (!c) throw new Error(`no card ${group}`);
    c.scrollIntoView({ block: 'center' });
    if (ignore) press(c, 'Ignore');
    await sleep(400);
    press(c, 'Send answer');
    await sleep(1200);
  };
  const onDay = async (needle) => {
    await sleep(1500);
    if (!text().includes(needle)) openTomorrow();
    await until(needle, () => text().includes(needle), 60000);
    show(needle);
  };

  try {
    await started();
    await signUp();
    await saveKey();
    await mark('shot 01-signed-in');
    await invoke('plugin:aroh-calendar|seed', { events: cfg.seed });

    await go('/settings');
    const label = document.querySelector('label[for$="-cal"]');
    document.getElementById(label.htmlFor).click();
    show('Read device calendars');
    await mark('shot 02-calendars-on');

    let found = false;
    for (let i = 0; i < 40 && !found; i++) {
      await go(i % 2 ? '/today' : '/inbox');
      await sleep(4000);
      found = location.pathname === '/inbox' && text().includes('Work meetings:');
    }
    if (!found) throw new Error('no triage cards');
    await mark('shot 03-triage-cards');
    await decide('Work meetings', false);
    await decide('Gym classes', true);
    await decide('Appointments', true);
    await decide('Holidays', false);
    await mark('shot 04-decided');

    await go('/calendar');
    await onDay('Team standup');
    await invoke('app_sync');
    await mark('shot 05-busy-on-calendar');

    await go('/goals/new');
    await say('Six pack by Feb 1');
    await until('question cards', () => text().includes('How many days'), 120000);
    await say('4');
    await say('No nuts');
    await until('plan', () => text().includes('Approve plan'), 180000);
    await mark('shot 06-plan-around-busy');
    button('Approve plan').click();
    await until('plan written', () => text().split('The plan is on your calendar').length > 2, 120000);
    await sleep(4000);

    await go('/calendar');
    await onDay('Core workout');
    await mark('shot 07-calendar-plan-and-busy');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
