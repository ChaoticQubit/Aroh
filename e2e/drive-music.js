(async () => {
  const { invoke, sleep, mark, text, until, go, button, show, say, started, signUp, saveKey, planGoal, fail } = window.AROH_LIB;
  const MUSIC = 'Learn Clair de Lune on piano by March';
  const snapshot = () => invoke('app_snapshot', { from: new Date(Date.now() - 864e5).toISOString(), to: new Date(Date.now() + 15 * 864e5).toISOString() });
  const poll = async (what, ok, ms) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      await invoke('app_sync');
      const found = await ok();
      if (found) return found;
      await sleep(3000);
    }
    throw new Error(`timed out waiting for ${what}`);
  };
  const chip = (name) => [...document.querySelectorAll('[aria-label="Filter by category"] button')].find((b) => b.textContent.trim() === name);
  const MUSIC_GOAL = { title: MUSIC, asks: 'How long have you played piano?', answers: ['Two years, Grade 4 pieces', '30'], plan: 'Piano practice' };

  try {
    await started();
    await signUp('shot 01-settings-account');
    await saveKey();
    await mark('shot 02-key-saved');
    await planGoal();
    await planGoal({ questions: 'shot 03-music-questions', plan: 'shot 04-music-plan', approved: 'shot 05-music-approved' }, 120000, MUSIC_GOAL);

    const goal = await poll('music goal synced', async () => (await snapshot()).goals.find((g) => g.title.includes('Clair de Lune') && g.category_id), 60000);
    await poll('music plugin', async () => (await invoke('plugins_list', { goalId: goal.id })).find((p) => p.enabled && p.tree && p.name === 'music'), 120000);
    const trackers = await poll('music trackers', async () => {
      const t = (await snapshot()).trackers.filter((x) => x.goal_id === goal.id);
      return ['Practice', 'Tempo', 'Pieces'].every((n) => t.some((x) => x.name === n)) && t;
    }, 60000);
    const id = (name) => trackers.find((t) => t.name === name).id;
    await invoke('app_add_entry', { trackerId: id('Practice'), data: { minutes: 25 } });
    await invoke('app_add_entry', { trackerId: id('Tempo'), data: { bpm: 48 } });
    await invoke('app_add_entry', { trackerId: id('Tempo'), data: { bpm: 54 } });
    await invoke('app_add_entry', { trackerId: id('Pieces'), data: { piece: 'Clair de Lune, bars 1 to 14', progress: 20 } });
    await invoke('app_add_entry', { trackerId: id('Pieces'), data: { piece: 'Clair de Lune, bars 15 to 26', progress: 35 } });

    await go(`/goals/${goal.id}`);
    await until('music dashboard', () => text().includes('Minutes practised') && text().includes('Tempo (bpm)'), 30000);
    await mark('shot 06-music-dashboard');
    show('Practice notes');
    await mark('shot 07-music-dashboard-pieces');
    show('Next sessions');
    await mark('shot 08-music-dashboard-upcoming');

    await go('/goals');
    await until('goal chips', () => chip('Hobby and Skill') && chip('Health and Fitness'), 15000);
    await mark('shot 09-goals-categories');
    chip('Health and Fitness').click();
    await until('fitness goal hidden', () => !text().includes('Six pack by Feb 1'), 5000);
    await mark('shot 10-goals-filtered');

    await go('/calendar');
    await until('calendar chips', () => chip('Hobby and Skill') && chip('Health and Fitness'), 15000);
    await mark('shot 11-calendar-categories');
    chip('Health and Fitness').click();
    await sleep(800);
    await mark('shot 12-calendar-filtered');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
