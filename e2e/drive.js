(async () => {
  const { cfg, invoke, sleep, mark, text, until, go, button, show, openTomorrow, say, started, signUp, saveKey, fail } = window.AROH_LIB;

  try {
    await started();
    if (!(await invoke('app_notification_permission'))) throw new Error('notifications not allowed');
    await signUp('shot 01-settings-account');
    await mark('shot 02-signed-in');

    await saveKey();
    await mark('shot 03-key-saved');

    await go('/goals/new');
    await say('Six pack by Feb 1');
    await until('question cards', () => text().includes('How many days'), 120000);
    await mark('shot 04-questions');
    await say('4');
    await say('No nuts');
    await until('plan', () => text().includes('Approve plan'), 120000);
    await mark('shot 05-plan');
    button('Approve plan').click();
    await until('plan written', () => text().split('The plan is on your calendar').length > 2, 120000);
    await sleep(4000);
    await mark('shot 06-approved');

    const synced = () => text().includes('Core workout') || text().includes('Morning plank');
    await go('/today');
    const today = await until('actions synced', synced, 15000).then(() => true, () => false);
    await mark('shot 07-today');
    await go('/calendar');
    await sleep(1000);
    if (!today) {
      openTomorrow();
      await until('actions synced', synced, 45000);
    }
    show('Core workout');
    await mark('shot 08-calendar');

    await go('/log');
    await until('check-in due', () => !!document.querySelector('[aria-label="Increase Weight"]'), 30000);
    document.querySelector('[aria-label="Increase Weight"]').click();
    await sleep(1500);
    await mark('shot 09-check-in');

    const snap = await invoke('app_snapshot', { from: new Date(Date.now() - 864e5).toISOString(), to: new Date(Date.now() + 864e5).toISOString() });
    const goal = snap.goals[0];
    await go(`/goals/new?goal=${goal.id}`);
    await say("I missed today's workout");
    await until('replan proposal', () => text().includes('in your Inbox to apply'), 120000);
    await mark('shot 10-replan');

    await go('/inbox');
    await until('proposal in inbox', () => text().includes('Make-up core workout'), 60000);
    await mark('shot 11-inbox');
    button('Apply').click();
    await until('applied', () => text().includes('Applied.'), 30000);
    await sleep(6000);
    await mark('shot 12-applied');

    await go('/calendar');
    await sleep(1000);
    openTomorrow();
    await sleep(1500);
    show('Make-up core workout');
    await mark('shot 13-calendar-replanned');
    await mark('background');
  } catch (e) {
    await fail(e);
  }
})();
