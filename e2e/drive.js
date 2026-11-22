(async () => {
  const cfg = window.AROH_E2E;
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args ?? {});
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
  const mark = async (text) => {
    await invoke('app_dev_mark', { mark: text });
    await sleep(2500);
  };
  const text = () => document.body.innerText;
  const until = async (what, ok, ms = 60000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      if (ok()) return;
      await sleep(300);
    }
    throw new Error(`timed out waiting for ${what}`);
  };
  const go = async (path) => {
    const a = document.createElement('a');
    a.href = path;
    document.body.append(a);
    a.click();
    a.remove();
    await until(`route ${path}`, () => location.pathname + location.search === path);
    await sleep(800);
  };
  const field = (label) => {
    const l = [...document.querySelectorAll('label')].find((x) => x.textContent.trim() === label);
    const el = l && document.getElementById(l.htmlFor);
    if (!el) throw new Error(`no field ${label}`);
    return el;
  };
  const type = (el, value, commit) => {
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    if (commit) el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const button = (name) => {
    const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === name || x.getAttribute('aria-label') === name);
    if (!b) throw new Error(`no button ${name}`);
    return b;
  };
  const show = (needle) => {
    const el = [...document.querySelectorAll('body *')].find((x) => x.children.length === 0 && x.textContent.includes(needle));
    if (el) el.scrollIntoView({ block: 'center' });
  };
  const say = async (message) => {
    const box = document.querySelector('textarea[placeholder="Message your personas"]');
    type(box, message, false);
    await sleep(200);
    button('Send').click();
    await sleep(1500);
  };

  try {
    await until('app rendered', () => text().includes('Calendar') && text().includes('Inbox'), 120000);
    await sleep(3000);
    await invoke('app_dev_mark', { mark: 'start' });
    if (!(await invoke('app_notification_permission'))) throw new Error('notifications not allowed');
    await go('/settings');
    type(field('Project URL'), cfg.supabaseUrl, true);
    await sleep(800);
    type(field('Anon key'), cfg.anonKey, true);
    await sleep(800);
    type(field('Server URL'), cfg.engineUrl, true);
    await sleep(800);
    type(field('Email'), cfg.email, false);
    await mark('shot 01-settings-account');
    for (let attempt = 0; attempt < 10 && !text().includes('Signed in.'); attempt++) {
      if (attempt > 0) await sleep(5000);
      type(field('Password'), cfg.password, false);
      button(text().includes('already') ? 'Sign in' : 'Create account').click();
      await sleep(500);
      await until('sign-in answer', () => text().includes('Signed in.') || text().includes('server answered') || text().includes('rror'), 120000);
    }
    await until('signed in', () => text().includes('Signed in.'), 5000);
    await mark('shot 02-signed-in');

    type(field('API key'), cfg.key, false);
    button('Test connection').click();
    await until('key test', () => text().includes('Connected.') || text().includes('refused'));
    await until('key status', () => text().includes(`ends ${cfg.key.slice(-4)}`));
    await mark('shot 03-key-saved');

    await go('/goals/new');
    await say('Six pack by Feb 1');
    await until('question cards', () => text().includes('How many days'), 120000);
    await mark('shot 04-questions');
    await say('4');
    await say('No nuts');
    await until('plan', () => text().includes('Approve plan'), 120000);
    await mark('shot 05-plan');
    button('Approve plan').click();
    await until('plan written', () => text().split('The plan is on your calendar').length > 2, 120000);
    await sleep(4000);
    await mark('shot 06-approved');

    await go('/today');
    await until('actions synced', () => text().includes('Core workout') || text().includes('Morning plank'), 60000);
    await mark('shot 07-today');
    await go('/calendar');
    await sleep(1000);
    show('Core workout');
    await mark('shot 08-calendar');

    await go('/log');
    await until('check-in due', () => !!document.querySelector('[aria-label="Increase Weight"]'), 30000);
    document.querySelector('[aria-label="Increase Weight"]').click();
    await sleep(1500);
    await mark('shot 09-check-in');

    const snap = await invoke('app_snapshot', { from: new Date(Date.now() - 864e5).toISOString(), to: new Date(Date.now() + 864e5).toISOString() });
    const goal = snap.goals[0];
    await go(`/goals/new?goal=${goal.id}`);
    await say("I missed today's workout");
    await until('replan proposal', () => text().includes('in your Inbox to apply'), 120000);
    await mark('shot 10-replan');

    await go('/inbox');
    await until('proposal in inbox', () => text().includes('Make-up core workout'), 60000);
    await mark('shot 11-inbox');
    button('Apply').click();
    await until('applied', () => text().includes('Applied.'), 30000);
    await sleep(6000);
    await mark('shot 12-applied');

    await go('/calendar');
    await sleep(1000);
    const tomorrow = new Date(Date.now() + 864e5);
    const day = tomorrow.toLocaleDateString('en-GB', { weekday: 'short' });
    [...document.querySelectorAll('button')].find((b) => b.textContent.includes(day) && b.textContent.includes(String(tomorrow.getDate())))?.click();
    await sleep(1500);
    show('Make-up core workout');
    await mark('shot 13-calendar-replanned');
    await mark('background');
  } catch (e) {
    await invoke('app_dev_mark', { mark: `fail ${String(e.message ?? e).replace(/[^\x20-\x7e]/g, '?').slice(0, 70)}` }).catch(() => {});
    await mark('shot 99-failure');
  }
})();
