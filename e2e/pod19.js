(async () => {
  const { invoke, sleep, mark, text, until, go, field, type, button, show, started, signUp, saveKey, planGoal, fail } = window.AROH_LIB;
  const SAFETY = 'Safety: no individual investment, tax or legal advice; refer to a licensed financial professional for those.';
  const INSTRUCTIONS = 'Answer in plain words. Quote the "50/30/20" rule when it fits.\nUse a \\ only for paths.\nEnd with one next step.';
  const RULE = 'Keep every answer under five sentences.';
  const section = (label) => document.querySelector(`section[aria-label="${label}"]`);
  const row = (scope, name) => [...(scope?.querySelectorAll('button') ?? [])].find((b) => b.textContent.includes(name));
  const top = async () => {
    document.querySelector('main')?.scrollTo(0, 0);
    await sleep(400);
  };

  try {
    await started();
    await signUp();
    await saveKey();
    await planGoal();

    await go('/settings');
    show('Coach library');
    await mark('shot 01-settings-coaches-link');
    row(document, 'Coach library').click();
    await until('library', () => location.pathname === '/personas' && text().includes('Fitness') && text().includes('Budget coach'), 60000);
    await sleep(800);
    await mark('shot 02-library-teams');

    row(section('Finance coaches'), 'Budget coach').click();
    await until('built-in coach', () => location.pathname === '/personas/budget-coach' && text().includes('Copy to edit'), 30000);
    await sleep(800);
    if (document.querySelector('textarea')) throw new Error('built-in coach is editable');
    await mark('shot 04-budget-coach-builtin');
    show('Rules');
    await mark('shot 05-builtin-rules-locked');

    button('Copy to edit').click();
    await until('copy', () => text().includes('Copied.') && text().includes('Save'), 30000);
    await top();
    await mark('shot 06-copied-editable');
    type(field('Name'), 'Money mentor', false);
    type(field('Instructions'), INSTRUCTIONS, false);
    type(document.querySelector('input[aria-label="New rule"]'), RULE, false);
    await sleep(300);
    button('Add').click();
    await sleep(500);
    if ([...document.querySelectorAll('button')].some((b) => (b.getAttribute('aria-label') ?? '').startsWith('Remove rule: Safety:'))) throw new Error('safety rule has a remove control');
    if (!text().includes(SAFETY) || !text().includes(RULE)) throw new Error('rules list incomplete');
    show(RULE);
    await mark('shot 07-edited-rule-added');

    button('Save').click();
    await sleep(500);
    const status = () => document.querySelector('[role="status"]')?.textContent ?? '';
    await until('saved', () => status() !== '' && status() !== 'Saving.', 30000);
    if (!status().startsWith('Saved.')) throw new Error(`save refused: ${status().slice(0, 120)}`);
    const saved = (await invoke('app_engine', { method: 'GET', path: '/v1/personas', body: null })).body.find((p) => p.name === 'budget-coach');
    if (saved.display_name !== 'Money mentor' || saved.instructions !== INSTRUCTIONS || !saved.guardrails.includes(SAFETY) || !saved.guardrails.includes(RULE)) throw new Error('saved copy differs');
    await top();
    await mark('shot 08-saved');

    button('Coaches').click();
    await until('library with copy', () => location.pathname === '/personas' && text().includes('Money mentor'), 30000);
    section('Finance coaches')?.scrollIntoView({ block: 'start' });
    await sleep(500);
    await mark('shot 09-library-your-copy');

    const goal = (await invoke('app_snapshot', { from: new Date(Date.now() - 864e5).toISOString(), to: new Date(Date.now() + 864e5).toISOString() })).goals[0];
    await go(`/goals/${goal.id}`);
    show('Coaches on this goal');
    await mark('shot 10-goal-coaches-link');
    button('Coaches on this goal').click();
    await until('goal coaches', () => location.pathname === '/personas' && text().includes(`Coaches on ${goal.title}`) && text().includes('Goal planner'), 30000);
    await sleep(800);
    await mark('shot 11-coaches-on-goal');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
