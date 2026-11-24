(async () => {
  const { invoke, sleep, mark, text, until, button, fail } = window.AROH_LIB;
  const note = (m) => invoke('app_dev_mark', { mark: m.replace(/[^\x20-\x7e]/g, '?').slice(0, 80) });
  const theme = async (t) => {
    await note(`appearance ${t}`);
    await until(`${t} scheme`, () => matchMedia('(prefers-color-scheme: dark)').matches === (t === 'dark'), 15000).catch(() => note(`result theme-live fail webview kept ${t === 'dark' ? 'light' : 'dark'}`));
    await sleep(1200);
  };
  try {
    const NEXT = ['Get started', 'Not now', 'Continue', 'Skip, show me the app'];
    const onboarding = () => text().includes('Goals that plan themselves') || /Step \d of 4/.test(text());
    await until('onboarding', onboarding, 120000);
    for (let step = 1; step <= 6 && onboarding(); step++) {
      await theme('light');
      await mark(`shot light/00-onboarding-${step}`);
      await theme('dark');
      await mark(`shot dark/00-onboarding-${step}`);
      const next = NEXT.find((name) => [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === name && !b.disabled));
      if (!next && !onboarding()) break;
      if (!next) throw new Error(`no way on from onboarding step ${step}`);
      button(next).click();
      await sleep(1200);
    }
    await theme('light');
    await until('app rendered', () => text().includes('Calendar') && text().includes('Dashboard'), 60000);
    await note(`result onboarding pass ${onboarding() ? 'still shown' : 'walked to the app'}`);
    if (!(await invoke('app_dev_seed'))) throw new Error('sample not loaded');
    await sleep(2000);
    await note('done');
  } catch (e) {
    await fail(e);
  }
})();
