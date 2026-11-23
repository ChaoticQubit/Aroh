(async () => {
  const { cfg, invoke, sleep, mark: note, text, until, go, started, fail } = window.AROH_LIB;
  const mark = async (m) => {
    await note(m);
    if (m.startsWith('shot ')) await sleep(3000);
  };
  const main = () => document.querySelector('main');
  const scrollTo = async (el) => {
    const m = main();
    m.scrollTop += el.getBoundingClientRect().top - m.getBoundingClientRect().top - 72;
    await sleep(800);
  };
  const chip = async (label) => {
    [...document.querySelectorAll('[aria-label="Filter goals"] button')].find((b) => b.textContent.trim() === label).click();
    await sleep(800);
  };
  const overflows = () => {
    const hits = [];
    const root = document.documentElement;
    if (root.scrollWidth > innerWidth + 1) hits.push(`html ${root.scrollWidth}>${innerWidth}`);
    for (const el of document.querySelectorAll('body *')) {
      if (String(el.className).includes('overflow-x-auto')) continue;
      const x = getComputedStyle(el).overflowX;
      if ((x === 'auto' || x === 'scroll') && el.scrollWidth > el.clientWidth + 1) hits.push(`${el.tagName.toLowerCase()} ${el.scrollWidth}>${el.clientWidth}`);
    }
    const wide = [...document.querySelectorAll('main *')].filter((el) => el.getBoundingClientRect().right > innerWidth + 1 && !el.closest('.overflow-x-auto'));
    if (wide.length) hits.push(`wide ${wide[0].tagName.toLowerCase()}`);
    return hits;
  };

  try {
    await started();
    await go('/dashboard');
    await until('empty dashboard', () => text().includes('No goals yet'), 45000);
    await mark(`shot ${cfg.prefix}-00-empty`);

    await invoke('app_dev_dashboard_seed');
    const { data } = await import('/src/lib/app/data.svelte.ts');
    await data.load();
    const save = data.snapshot.goals.find((g) => g.title === 'Save $10k');
    await invoke('plugins_use_template', { goalId: save.id, name: 'finance' });
    await data.load();
    const savings = data.snapshot.trackers.find((t) => t.goal_id === save.id && t.name === 'Savings');
    await invoke('app_add_entry', { trackerId: savings.id, data: { amount: 3200 } });
    await data.load();
    await until('seeded dashboard', () => text().includes('Minutes per week') && text().includes('Run a half marathon'), 30000);
    await sleep(1500);
    main().scrollTop = 0;
    await sleep(500);
    await invoke('app_dev_mark', { mark: `overflow ${overflows().length} ${overflows()[0] ?? ''}`.slice(0, 80) });
    await mark(`shot ${cfg.prefix}-01-dashboard`);

    await scrollTo(document.querySelector('#dash-minutes'));
    await mark(`shot ${cfg.prefix}-02-minutes-by-goal`);
    [...document.querySelectorAll('[aria-label="Stack by"] button')].find((b) => b.textContent.trim() === 'Category').click();
    await sleep(800);
    await mark(`shot ${cfg.prefix}-03-minutes-by-category`);
    const bars = [...document.querySelectorAll('svg rect[aria-label^="Week of"]')];
    bars[bars.length - 2].dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await sleep(800);
    await mark(`shot ${cfg.prefix}-03b-week-picked`);

    await scrollTo(document.querySelector('#dash-trends'));
    await mark(`shot ${cfg.prefix}-04-trends`);
    await scrollTo(document.querySelector('#dash-trends ~ ul li:last-child') ?? document.querySelector('#dash-trends'));
    await invoke('app_dev_mark', { mark: `target ${text().includes('of 5,000') || text().includes('of 5000') ? 'shown' : 'missing'}` });
    await mark(`shot ${cfg.prefix}-04b-trend-target`);

    main().scrollTop = 0;
    await chip('Behind');
    await invoke('app_dev_mark', { mark: `overflow-behind ${overflows().length}` });
    await mark(`shot ${cfg.prefix}-05-behind`);
    await chip('Ahead');
    await mark(`shot ${cfg.prefix}-06-ahead`);
    await chip('Learning');
    await scrollTo(document.querySelector('#dash-minutes'));
    await mark(`shot ${cfg.prefix}-07-learning-minutes`);
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
