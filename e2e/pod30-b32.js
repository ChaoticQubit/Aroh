(async () => {
  const { invoke, sleep, mark, until, go, started, fail } = window.AROH_LIB;
  const note = (m) => invoke('app_dev_mark', { mark: m });
  const errors = [];
  addEventListener('error', (e) => errors.push(String(e?.error?.stack ?? e?.message ?? e)));
  addEventListener('unhandledrejection', (e) => errors.push(String(e?.reason?.stack ?? e?.reason ?? e)));
  const rows = () => [...document.querySelectorAll('main li > button')];
  const visible = () => {
    const box = document.querySelector('main').getBoundingClientRect();
    return rows().some((r) => {
      const b = r.getBoundingClientRect();
      return b.height > 0 && b.bottom > box.top && b.top < box.bottom;
    });
  };
  try {
    await started();
    await note(`seeded ${await invoke('app_dev_seed')}`);
    await go('/today');
    await go('/goals');
    await until('goal rows', () => rows().length > 0, 30000);
    rows()[0].click();
    await until('goal screen', () => /^\/goals\/[^/]+$/.test(location.pathname), 15000);
    const href = location.pathname;
    for (const scrolled of [false, true]) {
      await go(href);
      await sleep(1500);
      const main = document.querySelector('main');
      if (scrolled) main.scrollTop = main.scrollHeight;
      await sleep(500);
      window.__b32 = 'kept';
      [...document.querySelectorAll('nav[aria-label="Main"] button, nav[aria-label="Main"] a')].find((b) => b.textContent.trim() === 'Goals').click();
      await sleep(2500);
      const m = document.querySelector('main');
      await note(`b32 ${scrolled ? 'scrolled' : 'top'} ${location.pathname} ${window.__b32 ?? 'reloaded'} rows=${rows().length} vis=${visible()}`);
      await note(`b32 main top=${Math.round(m.scrollTop)} h=${m.scrollHeight} ch=${m.clientHeight}`);
      await mark(`shot b32-goals-tab-${scrolled ? 'scrolled' : 'top'}`);
    }
    for (const e of errors.slice(0, 3)) for (let i = 0; i < Math.min(e.length, 280); i += 70) await note(`err ${e.slice(i, i + 70).replace(/[^\x20-\x7e]/g, '?')}`);
    await note('done');
  } catch (e) {
    await fail(e);
  }
})();
