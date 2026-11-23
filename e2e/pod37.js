(async () => {
  const { sleep, mark: note, until, go } = window.AROH_LIB;
  const mark = async (m) => {
    await note(m);
    if (m.startsWith('shot ')) await sleep(2500);
  };
  const SHOTS = ['01-leg-day', '02-logging', '03-chat-diff-undo', '04-scope-choice', '05-meal-prep', '06-week3-outline'];
  const LONG = [0, 1, 4];
  const overflows = () => {
    const hits = [];
    if (document.documentElement.scrollWidth > innerWidth + 1) hits.push(`html ${document.documentElement.scrollWidth}`);
    const wide = [...document.querySelectorAll('main *')].filter((el) => el.getBoundingClientRect().right > innerWidth + 1);
    if (wide.length) hits.push(`wide ${wide[0].tagName.toLowerCase()} ${Math.round(wide[0].getBoundingClientRect().right)}`);
    return hits;
  };
  try {
    await until('app hydrated', () => document.body.innerText.trim().length > 0);
    await sleep(6000);
    await go('/mockups');
    for (const theme of ['light', 'dark']) {
      for (let i = 0; i < SHOTS.length; i++) {
        location.hash = `action-cards/${i}/${theme}`;
        await sleep(900);
        const main = document.querySelector('main');
        main.scrollTop = 0;
        await sleep(300);
        const o = overflows();
        await note(`overflow ${SHOTS[i]} ${o.length} ${o[0] ?? ''}`);
        await mark(`shot ${theme}/${SHOTS[i]}`);
        if (LONG.includes(i)) {
          main.scrollTop = i === 1 ? 700 : main.scrollHeight;
          await sleep(600);
          await mark(`shot ${theme}/${SHOTS[i]}-scrolled`);
        }
      }
    }
    await mark('done');
  } catch (e) {
    await mark(`shot 99-failure ${String(e).slice(0, 80)}`);
  }
})();
