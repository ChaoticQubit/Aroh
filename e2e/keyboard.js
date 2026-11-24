(async () => {
  const { sleep, mark, text, until, go, fail, skipSetup, invoke } = window.AROH_LIB;
  const tabBar = () => {
    const nav = document.querySelector('nav[aria-label="Main"]');
    const r = nav ? nav.getBoundingClientRect() : null;
    return r && r.height > 0 && getComputedStyle(nav).display !== 'none' ? `${Math.round(r.top)}-${Math.round(r.bottom)}` : 'hidden';
  };
  const report = async (label) => {
    const vv = window.visualViewport;
    await invoke('app_dev_mark', { mark: `kb ${label} vv=${Math.round(vv.height)}@${Math.round(vv.offsetTop)} inner=${window.innerHeight} scrollY=${Math.round(window.scrollY)} tabbar=${tabBar()} typing=${document.documentElement.hasAttribute('data-typing')}` });
  };
  const focusOn = async (selector, name) => {
    const el = document.querySelector(selector);
    if (!el) throw new Error(`no ${selector}`);
    el.focus();
    for (const ms of [100, 250, 400, 700]) {
      await sleep(ms === 100 ? 100 : ms - [100, 250, 400, 700][[100, 250, 400, 700].indexOf(ms) - 1]);
      await report(`${name} +${ms}ms`);
      if (ms === 250) await mark(`shot ${name}-a`);
    }
    await sleep(800);
    await report(`${name} settled`);
    await mark(`shot ${name}-b`);
    el.blur();
    await sleep(800);
  };

  try {
    await until('onboarding', () => text().includes('Get started'), 120000);
    await skipSetup();
    await sleep(1500);
    await report('rest');
    await go('/goals/new');
    await sleep(1500);
    await focusOn('input[aria-required="true"]', 'kb-form');
    await go('/log');
    await sleep(1500);
    await focusOn('textarea, input:not([type=hidden])', 'kb-log');
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
