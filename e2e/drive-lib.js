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
  const button = (name) => {
    const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === name || x.getAttribute('aria-label') === name);
    if (!b) throw new Error(`no button ${name}`);
    return b;
  };
  const show = (needle) => {
    const el = [...document.querySelectorAll('body *')].find((x) => x.children.length === 0 && x.textContent.includes(needle));
    if (el) el.scrollIntoView({ block: 'center' });
  };
  const openTomorrow = () => {
    const tomorrow = new Date(Date.now() + 864e5);
    const day = tomorrow.toLocaleDateString('en-GB', { weekday: 'short' });
    [...document.querySelectorAll('button')].find((b) => b.textContent.includes(day) && b.textContent.includes(String(tomorrow.getDate())))?.click();
  };
  const say = async (message) => {
    const box = document.querySelector('textarea[placeholder="Message your personas"]');
    type(box, message, false);
    await sleep(200);
    button('Send').click();
    await sleep(1500);
  };
  const started = async () => {
    await until('app rendered', () => text().includes('Calendar') && text().includes('Inbox'), 120000);
    await sleep(3000);
    await invoke('app_dev_mark', { mark: 'start' });
  };
  const signUp = async (shot) => {
    await go('/settings');
    type(field('Project URL'), cfg.supabaseUrl, true);
    await sleep(800);
    type(field('Anon key'), cfg.anonKey, true);
    await sleep(800);
    type(field('Server URL'), cfg.engineUrl, true);
    await sleep(800);
    type(field('Email'), cfg.email, false);
    if (shot) await mark(shot);
    for (let attempt = 0; attempt < 10 && !text().includes('Signed in.'); attempt++) {
      if (attempt > 0) await sleep(5000);
      type(field('Password'), cfg.password, false);
      button(text().includes('already') ? 'Sign in' : 'Create account').click();
      await sleep(500);
      await until('sign-in answer', () => text().includes('Signed in.') || text().includes('server answered') || text().includes('rror'), 120000);
    }
    await until('signed in', () => text().includes('Signed in.'), 5000);
  };
  const saveKey = async () => {
    type(field('API key'), cfg.key, false);
    button('Test connection').click();
    await until('key test', () => text().includes('Connected.') || text().includes('refused'));
    await until('key status', () => text().includes(`ends ${cfg.key.slice(-4)}`));
  };
  const fail = async (e) => {
    await invoke('app_dev_mark', { mark: `fail ${String(e.message ?? e).replace(/[^\x20-\x7e]/g, '?').slice(0, 70)}` }).catch(() => {});
    await mark('shot 99-failure');
  };
  return { cfg, invoke, sleep, mark, text, until, go, field, type, button, show, openTomorrow, say, started, signUp, saveKey, fail };
})();
