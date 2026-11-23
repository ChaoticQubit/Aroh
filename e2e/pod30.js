(async () => {
  const { sleep, mark, go, until } = window.AROH_LIB;
  const SHOTS = ['01-plan-week1', '02-plan-tap-session', '03-plan-week2-busy-day', '04-plan-week4-finish', '05-cant-fit', '06-places-back', '07-dashboard', '08-dashboard-behind'];
  try {
    await until('app hydrated', () => document.body.innerText.trim().length > 0);
    await sleep(6000);
    await go('/mockups');
    for (const theme of ['light', 'dark']) {
      for (let i = 0; i < SHOTS.length; i++) {
        location.hash = `plan-view/${i}/${theme}`;
        await sleep(700);
        document.querySelector('main').scrollTop = 0;
        await sleep(300);
        await mark(`shot ${theme}/${SHOTS[i]}`);
        if (i === 6) {
          const main = document.querySelector('main');
          main.scrollTop = main.scrollHeight;
          await sleep(600);
          await mark(`shot ${theme}/07b-dashboard-charts`);
        }
      }
    }
    await mark('done');
  } catch (e) {
    await mark(`shot 99-failure ${String(e).slice(0, 80)}`);
  }
})();
