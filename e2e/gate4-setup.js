(async () => {
  const { invoke, sleep, mark, text, until, button, fail } = window.AROH_LIB;
  const note = (m) => invoke('app_dev_mark', { mark: m.replace(/[^\x20-\x7e]/g, '?').slice(0, 80) });
  const theme = async (t) => {
    await note(`appearance ${t}`);
    await until(`${t} scheme`, () => matchMedia('(prefers-color-scheme: dark)').matches === (t === 'dark'), 15000).catch(() => note(`result theme-live fail webview kept ${t === 'dark' ? 'light' : 'dark'}`));
    await sleep(1200);
  };
  try {
    await until('first-launch setup', () => text().includes('Skip for now'), 120000);
    for (let step = 1; step <= 3 && text().includes('Skip for now'); step++) {
      await theme('light');
      await mark(`shot light/00-setup-${step}`);
      await theme('dark');
      await mark(`shot dark/00-setup-${step}`);
      button('Skip for now').click();
      await sleep(1200);
    }
    await theme('light');
    await until('app rendered', () => text().includes('Calendar') && text().includes('Dashboard'), 60000);
    await note(`result setup pass ${text().includes('Skip for now') ? 'still shown' : 'three steps skipped'}`);
    if (!(await invoke('app_dev_seed'))) throw new Error('sample not loaded');
    await sleep(2000);
    await note('done');
  } catch (e) {
    await fail(e);
  }
})();
