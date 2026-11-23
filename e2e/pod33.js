(async () => {
  const { invoke, sleep, mark, text, until, go, button, show, started, skipSetup, fail } = window.AROH_LIB;
  const check = (ok, what) => {
    if (!ok) throw new Error(what);
  };
  const snapshot = () => invoke('app_snapshot', { from: new Date(Date.now() - 30 * 864e5).toISOString(), to: new Date(Date.now() + 30 * 864e5).toISOString() });
  const goalNamed = (s, title) => s.goals.find((g) => g.title === title);
  const deleteFrom = async (goal, choice, shot) => {
    await go(`/goals/${goal.id}`);
    await until('delete entry', () => text().includes('Delete goal'), 60000);
    show('Delete goal');
    if (shot) {
      await mark(`shot ${shot}`);
      await sleep(5000);
    }
    button('Delete goal').click();
    await until('sheet', () => text().includes('Delete goal and its actions') && text().includes('Delete goal, keep actions'));
    await sleep(800);
    if (shot) await mark('shot 02-delete-sheet');
    button(choice).click();
    await until('back on Goals', () => location.pathname === '/goals', 30000);
    await sleep(1500);
  };

  try {
    await started();
    await invoke('app_dev_seed');
    await sleep(2000);
    await skipSetup();
    let s = await snapshot();
    const six = goalNamed(s, 'Six pack by Feb 1');
    const piano = goalNamed(s, 'Learn piano');
    check(six && piano, 'sample goals missing');
    const sixActions = s.actions.filter((a) => a.goal_id === six.id).length;
    const pianoActions = s.actions.filter((a) => a.goal_id === piano.id).map((a) => a.id);
    check(sixActions > 0 && pianoActions.length > 0, 'sample actions missing');

    await deleteFrom(six, 'Delete goal and its actions', '01-goal-screen-delete-entry');
    s = await snapshot();
    check(!goalNamed(s, 'Six pack by Feb 1'), 'six pack goal still listed');
    check(!s.actions.some((a) => a.goal_id === six.id), 'six pack actions left');
    check(!s.trackers.some((t) => t.goal_id === six.id), 'six pack trackers left');
    await mark('shot 03-deleted-with-actions-goals');

    await deleteFrom(piano, 'Delete goal, keep actions', null);
    s = await snapshot();
    check(!goalNamed(s, 'Learn piano'), 'piano goal still listed');
    const routines = goalNamed(s, 'Routines');
    check(routines, 'no Routines goal');
    const kept = s.actions.filter((a) => pianoActions.includes(a.id));
    check(kept.length === pianoActions.length && kept.every((a) => a.goal_id === routines.id), 'piano actions not kept under Routines');
    await mark('shot 04-deleted-kept-actions-goals');
    await go('/today');
    await until('kept action on Today', () => text().includes('Piano practice'), 30000);
    show('Piano practice');
    await mark('shot 05-kept-actions-today');

    await go(`/goals/${routines.id}`);
    await until('Routines screen', () => text().includes('Routines'), 30000);
    await sleep(3000);
    check(!text().includes('Delete goal'), 'Routines offers delete');
    await mark('shot 06-routines-no-delete');
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
