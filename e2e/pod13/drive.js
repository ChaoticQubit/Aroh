(async () => {
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args ?? {});
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
  const mark = async (text) => {
    await invoke('app_dev_mark', { mark: text });
    await sleep(6000);
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
    await until(`route ${path}`, () => location.pathname === path);
    await sleep(800);
  };
  const show = (needle) => {
    const el = [...document.querySelectorAll('body *')].find((x) => x.children.length === 0 && x.textContent.includes(needle));
    if (el) el.scrollIntoView({ block: 'center' });
  };

  try {
    await until('app rendered', () => text().includes('Calendar') && text().includes('Inbox'), 120000);
    await sleep(2000);
    await invoke('app_dev_mark', { mark: 'start' });
    if (!(await invoke('app_notification_permission'))) throw new Error('notifications not allowed');
    await invoke('app_dev_health_seed');
    await go('/settings');
    show('Apple Health');
    const toggle = document.querySelector('[id$="-health"]');
    if (!toggle) throw new Error('no Apple Health switch');
    toggle.click();
    await until('health import', () => text().includes('Connected.'), 60000);
    if (!text().includes('Imported')) throw new Error(`nothing imported: ${text().match(/Connected\.[^\n]*/)}`);
    show('Connected.');
    await mark('shot 02-settings-connected');
    await go('/log');
    show('Recent entries');
    await until('health entries', () => text().includes('Apple Health'));
    await mark('shot 03-log-health-entries');
    const snap = await invoke('app_snapshot', { from: new Date(Date.now() - 864e5).toISOString(), to: new Date(Date.now() + 864e5).toISOString() });
    const health = snap.entries.filter((e) => e.source === 'health');
    await invoke('app_dev_mark', { mark: `health entries ${health.length}` });
    const goal = snap.goals[0];
    await go(`/goals/${goal.id}`);
    show('Steps');
    await mark('shot 04-goal-health');
    const probe = await invoke('app_dev_push_probe').then((v) => v, (e) => `error ${e}`);
    await invoke('app_dev_mark', { mark: `push ${String(probe).replace(/[^ -~]/g, '').slice(0, 80)}` });
    await invoke('app_dev_mark', { mark: 'background' });
  } catch (e) {
    await invoke('app_dev_mark', { mark: `error ${String(e).replace(/[^ -~]/g, '').slice(0, 60)}` });
    await mark('shot 99-failure');
  }
})();
