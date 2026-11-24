window.AROH_LIB = (() => {
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
  const keys = async (el, value) => {
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
    el.focus();
    document.execCommand('selectAll');
    document.execCommand('delete');
    if (el.value) {
      setter.call(el, '');
      el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'deleteContentBackward' }));
    }
    await sleep(100);
    let forced = 0;
    for (const ch of value) {
      el.dispatchEvent(new KeyboardEvent('keydown', { key: ch, bubbles: true, cancelable: true }));
      const before = el.value;
      if (!document.execCommand('insertText', false, ch) || el.value === before) {
        forced++;
        setter.call(el, before + ch);
        el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: ch }));
      }
      el.dispatchEvent(new KeyboardEvent('keyup', { key: ch, bubbles: true }));
      await sleep(40);
    }
    await sleep(300);
    return { value: el.value, forced };
  };
  const button = (name) => {
    const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === name || x.getAttribute('aria-label') === name);
    if (!b) throw new Error(`no button ${name}`);
    return b;
  };
  const show = (needle) => {
    const seen = (x) => {
      const r = x.getBoundingClientRect();
      return r.width > 4 && r.height > 4;
    };
    const el = [...document.querySelectorAll('body *')].find((x) => x.children.length === 0 && x.textContent.includes(needle) && seen(x));
    if (el) el.scrollIntoView({ block: 'center' });
  };
  const openTomorrow = async () => {
    const tomorrow = new Date(Date.now() + 864e5);
    const day = tomorrow.toLocaleDateString('en-GB', { weekday: 'short' });
    const find = () => [...document.querySelectorAll('button')].find((b) => b.textContent.includes(day) && b.textContent.includes(String(tomorrow.getDate())));
    if (!find()) {
      document.querySelector('button[aria-label="Next"]')?.click();
      await sleep(1000);
    }
    find()?.click();
  };
  const say = async (message) => {
    const box = document.querySelector('textarea[placeholder="Message your personas"]');
    type(box, message, false);
    await sleep(200);
    button('Send').click();
    await sleep(1500);
  };
  const useLocalStack = async () => {
    const local = { supabase_url: cfg.supabaseUrl, supabase_anon_key: cfg.anonKey, server_url: cfg.engineUrl };
    for (const [name, value] of Object.entries(local)) if (value) await invoke('app_set_setting', { name, value });
  };
  const ONBOARDING = ['Get started', 'Not now', 'Continue', 'Add later', 'Skip, show me the app'];
  const onboarding = () => text().includes('Goals that plan themselves') || /Step \d of 4/.test(text());
  const skipSetup = async () => {
    for (let step = 0; step < 8 && onboarding(); step++) {
      const next = ONBOARDING.find((name) => [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === name && !b.disabled));
      if (!next) break;
      button(next).click();
      await sleep(700);
    }
  };
  const settled = async () => {
    let calm = 0;
    for (let i = 0; i < 120 && calm < 6; i++) {
      if (onboarding()) {
        calm = 0;
        await skipSetup();
      } else if (text().includes('Calendar') && text().includes('Dashboard')) calm++;
      else calm = 0;
      await sleep(500);
    }
    if (calm < 6) throw new Error('app did not settle past onboarding');
  };
  const started = async () => {
    await useLocalStack();
    await until('app or onboarding rendered', () => onboarding() || (text().includes('Calendar') && text().includes('Dashboard')), 120000);
    await settled();
    await invoke('app_dev_mark', { mark: 'start' });
  };
  const signUp = async (shot) => {
    await useLocalStack();
    await go('/settings');
    await until('settings', () => text().includes('Sign in or create an account') || text().includes('Sign out'));
    if (text().includes('Sign out')) return;
    button('Sign in or create an accountAgents and sync need an account.').click();
    await until('onboarding', onboarding);
    button('Get started').click();
    await sleep(700);
    button('Continue with email').click();
    await sleep(700);
    type(field('Email'), cfg.email, false);
    if (shot) await mark(shot);
    for (let attempt = 0; attempt < 10 && !text().includes('Add your Claude key'); attempt++) {
      if (attempt > 0) await sleep(5000);
      type(field('Password'), cfg.password, false);
      await sleep(300);
      button(text().includes('already') ? 'Sign in' : 'Create account').click();
      await until('sign-in answer', () => text().includes('Add your Claude key') || text().includes('server answered') || text().includes('rror'), 120000);
    }
    await until('signed in', () => text().includes('Add your Claude key'), 5000);
  };
  const saveKey = async () => {
    if (text().includes('Add your Claude key')) {
      type(field('Claude API key'), cfg.key, false);
      await sleep(300);
      button('Save key').click();
      await until('key saved', () => text().includes('Your goals and the AI') || text().includes('refused'), 120000);
      button('I agree').click();
      await until('consent', () => text().includes('What Aroh may use'), 60000);
      await skipSetup();
      await until('app rendered', () => text().includes('Calendar') && text().includes('Dashboard'), 60000);
      return;
    }
    await go('/settings/key');
    await until('key page', () => text().includes('Monthly budget'));
    type(field('API key'), cfg.key, false);
    await sleep(300);
    button('Save and test').click();
    await until('key test', () => text().includes('Connected.') || text().includes('refused'));
    await until('key status', () => text().includes(`ends ${cfg.key.slice(-4)}`));
  };
  const SIX_PACK = { title: 'Six pack by Feb 1', asks: 'How many days', answers: ['4', 'No nuts'], plan: 'Approve plan' };
  const createGoal = async (title, description, shot) => {
    await go('/goals/new');
    await until('new goal form', () => text().includes('Create goal'));
    type(field('Title'), title, false);
    if (description) type(field('Description'), description, false);
    await sleep(1200);
    if (shot) await mark(shot);
    button('Create goal').click();
    await until('goal card', () => /^\/goals\/[0-9a-f-]{36}$/.test(location.pathname), 60000);
    await sleep(800);
    return location.pathname.split('/').pop();
  };
  const planGoal = async (shots = {}, planMs = 120000, goal = SIX_PACK) => {
    await go('/today');
    await createGoal(goal.title, goal.description, shots.form);
    await until('question cards', () => text().includes(goal.asks), 120000);
    if (shots.questions) await mark(shots.questions);
    for (const answer of goal.answers) await say(answer);
    button('Send answers').click();
    await until('plan', () => text().includes('Approve plan') && text().includes(goal.plan), planMs);
    if (shots.plan) await mark(shots.plan);
    button('Approve plan').click();
    await until('plan written', () => text().includes('The plan is on your calendar'), 120000);
    await sleep(4000);
    if (shots.approved) await mark(shots.approved);
  };
  const fail = async (e) => {
    await invoke('app_dev_mark', { mark: `fail ${String(e.message ?? e).replace(/[^\x20-\x7e]/g, '?').slice(0, 70)}` }).catch(() => {});
    await mark('shot 99-failure');
  };
  return { cfg, invoke, sleep, mark, text, until, go, field, type, keys, button, show, openTomorrow, say, started, signUp, saveKey, createGoal, planGoal, fail, useLocalStack, skipSetup, settled };
})();
