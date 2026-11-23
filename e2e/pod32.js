window.AROH_E2E = window.AROH_E2E ?? {};
(async () => {
  const { invoke, sleep, mark, text, until, go, button, fail } = window.AROH_LIB;
  const note = (m) => invoke('app_dev_mark', { mark: m.replace(/[^\x20-\x7e]/g, '?').slice(0, 80) }).catch(() => {});
  const mic = () => document.querySelector('button[aria-label="Speak a message"], button[aria-label="Stop speaking"]');
  const alertText = () => [...document.querySelectorAll('[role="alert"]')].map((a) => a.textContent.trim()).join(' | ');
  window.addEventListener('error', (e) => note(`js error ${e.message}`));
  window.addEventListener('unhandledrejection', (e) => note(`js rejection ${String(e.reason?.message ?? e.reason)}`));

  try {
    await until('app or setup rendered', () => text().includes('Skip for now') || text().includes('Inbox'), 120000);
    for (let i = 0; i < 40 && !(text().includes('Inbox') && !text().includes('Skip for now')); i++) {
      if (text().includes('Skip for now')) button('Skip for now').click();
      await sleep(750);
    }
    await invoke('app_dev_seed');
    const today = new Date().toISOString().slice(0, 10);
    const snap = await invoke('app_snapshot', { from: `${today}T00:00:00Z`, to: `${today}T23:59:59Z` });
    const goal = snap.goals.find((g) => !g.deleted);
    if (!goal) throw new Error('no sample goal');
    const channel = window.__TAURI_INTERNALS__.transformCallback(() => {});
    const listened = await invoke('plugin:aroh-speech|register_listener', { event: 'partial', handler: `__CHANNEL__:${channel}` }).then(() => 'ok', (e) => JSON.stringify(e));
    await note(`register_listener ${listened}`);
    if (listened !== 'ok') throw new Error(`partial listener refused: ${listened}`);
    await invoke('plugin:aroh-speech|remove_listener', { event: 'partial', channelId: channel }).catch(() => {});

    await go('/today');
    const quick = () => document.querySelector('button[aria-label="Speak a log entry"], form button[aria-label="Stop speaking"]');
    await until('quick-log mic on Today', () => quick() !== null, 30000);
    if (document.querySelector('button[aria-label="Log with a photo"]')) throw new Error('dead photo button still on Today');
    if (window.AROH_P32_MIC === 'deny') {
      quick().click();
      await sleep(2500);
      await note(`today mic alert=${alertText()}`);
      if (!alertText().includes('Microphone is off for Aroh')) throw new Error('Today mic shows no denied state');
      await mark('shot 09-today-mic-denied');
      await sleep(4000);
    } else {
      await mark('shot 09-today-quick-log');
      await sleep(4000);
    }

    await go('/goals/new');
    await until('new goal form', () => text().includes('Attachments'), 30000);
    const labels = [...document.querySelectorAll('button')].map((b) => b.textContent.trim()).filter((t) => ['Photo', 'File', 'Link'].includes(t));
    await note(`attach buttons ${labels.join(',')}`);
    document.querySelector('[aria-labelledby$="-att"]')?.scrollIntoView({ block: 'center' });
    await mark('shot 05-new-goal-attach');

    await go(`/goals/${goal.id}`);
    await until('goal card composer', () => mic() !== null, 30000);
    mic().scrollIntoView({ block: 'center' });
    await mark('shot 01-goal-card-mic');

    mic().click();
    await sleep(300);
    await note(`mic after tap pressed=${mic()?.getAttribute('aria-pressed')} label=${mic()?.getAttribute('aria-label')}`);
    await mark('shot 02-mic-permission-prompt');
    await sleep(8000);
    await note(`mic after wait pressed=${mic()?.getAttribute('aria-pressed')} alert=${alertText()}`);
    await mark('shot 03-mic-after-wait');
    const settings = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Open Settings');
    if (settings) {
      await note('burst 03b-settings-opened');
      await note('done');
      settings.click();
      return;
    }
    if (mic()?.getAttribute('aria-pressed') === 'true') {
      mic().click();
      await sleep(3000);
      await note(`mic after stop pressed=${mic()?.getAttribute('aria-pressed')} alert=${alertText()}`);
    }
    await mark('shot 04-mic-after-stop');

    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
