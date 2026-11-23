(async () => {
  const { sleep, mark, text, until, go, field, type, button, show, say, started, signUp, saveKey, fail, useLocalStack, skipSetup, invoke } = window.AROH_LIB;
  const GOAL = 'Finish reading The Joy of X by next Sunday';
  const DUNE = 'Finish reading Dune by 31 December';
  const open = async (path) => {
    await go(path);
    await sleep(1200);
  };
  const create = async () => {
    button('Create goal').click();
    await until('goal card', () => /^\/goals\/[0-9a-f-]{36}$/.test(location.pathname), 60000);
    await sleep(800);
  };

  try {
    await useLocalStack();
    await until('first-launch setup', () => text().includes('Skip for now'), 120000);
    await skipSetup();
    await started();
    await signUp();
    await saveKey();

    await open('/goals/new');
    await until('new goal form', () => text().includes('Create goal'));
    await mark('shot 01-form');
    type(field('Title'), GOAL, false);
    type(field('Description'), 'Thirty short chapters, I read in the evening.', false);
    await until('category detected', () => text().includes('Detected from the title'), 10000);
    button('Link').click();
    await sleep(300);
    type(document.querySelector('input[aria-label="Link"]'), 'https://example.org/joy-of-x', false);
    button('Add').click();
    await until('link listed', () => text().includes('example.org/joy-of-x'), 5000);
    await sleep(800);
    show('Attachments');
    await mark('shot 02-form-filled');
    window.scrollTo(0, 0);
    await create();
    await until('status card', () => !!document.querySelector('[aria-label="Agent status"]'), 30000);
    await until('link on the card', () => text().includes('1 attachment') && text().includes('Link · agents read it'), 30000);
    await mark('shot 03-card-working');

    await until('questions in the card', () => text().includes('How many chapters') && text().includes('Send answers'), 120000);
    await sleep(1500);
    show('Waiting for you');
    await mark('shot 04-card-questions');
    await say('4');
    await say('30');
    button('Skip question 3').click();
    await sleep(600);
    button('Send answers').click();
    await until('plan in the card', () => text().includes('Approve plan') && text().includes('three chapters each evening'), 120000);
    show('Approve plan');
    await mark('shot 05-card-plan');
    button('Approve plan').click();
    await until('plan written', () => text().includes('The plan is on your calendar'), 120000);
    await until('on track', () => text().includes('Agents are idle'), 60000);
    await sleep(3000);
    if (!text().includes('Research found')) throw new Error('no research summary on the card');
    if (!text().includes('Your answers')) throw new Error('no answers section on the card');
    window.scrollTo(0, 0);
    await mark('shot 06-card-on-track');
    show('Upcoming');
    await mark('shot 07-card-upcoming');

    await say('I read on the train on weekdays');
    await until('chat reply in the card', () => text().includes('in your Inbox to apply'), 120000);
    show('I read on the train');
    await mark('shot 08-card-chat');
    if (!text().includes('Research found') || !text().includes('Your answers')) throw new Error('research or answers left the card after a replan');
    document.querySelector('textarea[placeholder="Message your personas"]').focus();
    await sleep(1500);
    await mark('shot 08b-card-keyboard');
    document.activeElement.blur();
    await sleep(800);

    [...document.querySelectorAll('button')].find((b) => b.textContent.trim().startsWith('Agents and coaches')).click();
    await until('agents sheet', () => text().includes('Agents on this goal'));
    await sleep(800);
    await mark('shot 09-agents-sheet');
    button('Done').click();

    await open('/settings');
    show('What new goals start with');
    await mark('shot 10-settings-agents');
    await until('fact count', () => text().includes('1 fact'), 30000);
    await open('/settings/profile');
    await until('kept fact', () => text().includes('Free minutes a day: 30'), 30000);
    if (text().includes('Reading time of day')) throw new Error('a skipped answer was kept');
    await mark('shot 11-profile');

    await open('/goals/new');
    type(field('Title'), DUNE, false);
    await sleep(1200);
    await create();
    await until('still true check', () => text().includes('Still true?'), 120000);
    await sleep(1500);
    show('Still true?');
    await mark('shot 12-still-true');
    button('Yes').click();
    await sleep(600);
    await mark('shot 13-still-true-yes');

    await open('/goals/new');
    type(field('Title'), 'Learn pottery basics', false);
    await sleep(800);
    if (text().includes('Detected from the title')) { button('Change').click(); await sleep(400); }
    button('New category').click();
    await sleep(400);
    type(field('New category'), 'Crafts', false);
    button('Color 4').click();
    await sleep(400);
    await mark('shot 14a-category-swatches');
    await sleep(400);
    button('Add').click();
    await until('custom category picked', () => text().includes('Your pick') || text().includes('Crafts'), 10000);
    await sleep(800);
    await mark('shot 14-custom-category');
    await create();
    await until('custom category on the card', () => text().includes('Crafts') && !!document.querySelector('[aria-label="Agent status"]'), 30000);
    await sleep(1500);
    await mark('shot 15-custom-category-card');
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
