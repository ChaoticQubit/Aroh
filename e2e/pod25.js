(async () => {
  const { invoke, sleep, mark, text, until, go, button, show, say, started, signUp, saveKey, createGoal, fail, useLocalStack } = window.AROH_LIB;
  const GOAL = 'Finish reading The Joy of X by next Sunday';
  const composer = () => document.querySelector('textarea[placeholder="Message your personas"]');

  try {
    await useLocalStack();
    await until('first-launch setup', () => text().includes('Skip for now'), 120000);
    await sleep(1500);
    await mark('shot 01-setup-notifications');
    await sleep(5000);
    button('Skip for now').click();
    await until('calendar step', () => text().includes('Your calendar'));
    await mark('shot 02-setup-calendar');
    await sleep(5000);
    button('Skip for now').click();
    await until('health step', () => text().includes('Apple Health'));
    await mark('shot 03-setup-health');
    await sleep(5000);
    button('Skip for now').click();
    await until('sign-in after setup', () => location.pathname === '/settings', 30000);
    await sleep(1200);
    await mark('shot 04-after-setup-settings');
    await started();
    await signUp();
    await saveKey();
    show('Time limit per run, minutes');
    await mark('shot 05-run-limit-setting');

    await createGoal(GOAL);
    if (document.querySelector('button[aria-label="Attach a file"]')) throw new Error('attach is still shown');
    if (!document.querySelector('button[aria-label="Speak a message"]')) throw new Error('no mic in the chat composer');
    composer().focus();
    await sleep(1200);
    await mark('shot 06-composer-focused');
    await until('question group', () => text().includes('How many chapters') && text().includes('Send answers'), 120000);
    if (!text().includes('chapters') || !text().includes('minutes per day')) throw new Error('number questions show no unit');
    await sleep(2500);
    await mark('shot 07-questions-with-units');
    await say('4');
    await say('30');
    for (const n of [3, 4, 5, 6]) button(`Skip question ${n}`).click();
    await sleep(600);
    if (!text().includes('2 of 6 answered, 4 skipped')) throw new Error(`status line: ${text().match(/\d of 6 answered[^\n]*/)?.[0]}`);
    show('Send answers');
    await mark('shot 08-answered-and-skipped');
    button('Send answers').click();
    await until('plan', () => text().includes('Approve plan') && text().includes('three chapters each evening'), 120000);
    await mark('shot 09-plan');
    button('Approve plan').click();
    await until('plan written', () => text().includes('The plan is on your calendar'), 120000);
    await sleep(3000);
    await mark('shot 10-approved');

    await go('/inbox');
    await mark('shot 11-inbox');
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
