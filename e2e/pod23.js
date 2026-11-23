(async () => {
  const { invoke, sleep, mark, text, until, go, type, button, show, started, fail } = window.AROH_LIB;
  const GYM = { lat: 37.776, lon: -122.417 };
  const POOL = { lat: 37.7605, lon: -122.4321 };
  const view = () => invoke('app_location_view');
  const check = (ok, what) => {
    if (!ok) throw new Error(what);
  };
  const card = (needle) => [...document.querySelectorAll('[data-slot=card]')].find((c) => c.textContent.includes(needle));
  const press = (root, label) => {
    const b = [...root.querySelectorAll('button')].find((x) => x.textContent.trim() === label);
    check(b, `no button ${label}`);
    b.click();
  };

  try {
    await started();
    await invoke('app_dev_seed');
    await go('/settings/location');
    await until('explainer', () => text().includes('Visits stay on this phone'));
    await mark('shot 01-explainer');
    await sleep(6000);
    button('Continue').click();
    await until('location on', () => text().includes('Location log'), 60000);
    await mark('shot 02-settings-location');

    await invoke('app_dev_location_seed', { kind: 'prompt', lat: GYM.lat, lon: GYM.lon });
    await go('/inbox');
    await until('place prompt', () => text().includes('New places'), 60000);
    await sleep(1500);
    await mark('shot 03-inbox-place-prompt');
    press(card('Is this'), 'Yes, link it');
    await sleep(2500);
    const named = await view();
    const gym = named.places[0];
    check(gym && gym.links.length === 1, 'prompt did not name and link the place');
    const goalId = gym.links[0].goal_id;
    await invoke('app_location_link', { placeId: gym.id, goalId, seriesId: null });

    await go('/settings/places');
    await until('places map', () => document.querySelector('figure img') !== null, 60000);
    await sleep(1500);
    await mark('shot 04-places-snapshot');
    const search = document.querySelector('input[type=search]');
    type(search, 'gym', false);
    await until('search results', () => text().includes('Results') && document.querySelectorAll('button').length > 0 && !text().includes('No places match'), 30000).catch(() => {});
    await sleep(1500);
    await mark('shot 04-places-search');
    type(search, '', false);

    await invoke('app_dev_location_seed', { kind: 'due', lat: GYM.lat, lon: GYM.lon });
    await go('/today');
    await sleep(3000);
    await mark(`locate ${GYM.lat},${GYM.lon}`);
    await sleep(105000);
    await mark('burst 05-nudge-banner');
    await sleep(100000);

    await mark('locate 37.7600,-122.4400');
    await invoke('app_dev_location_seed', { kind: 'pattern', lat: POOL.lat, lon: POOL.lon });
    await go('/inbox');
    await until('learned routine', () => text().includes('Make it a weekly action?'), 60000);
    show('Make it a weekly action?');
    await sleep(1000);
    await mark('shot 07-inbox-learned-routine');
    press(card('Make it a weekly action?'), 'Apply');
    await sleep(3000);
    const learned = await view();
    check(learned.routines.some((r) => r.title === 'Swim laps'), 'approved routine is missing');

    await go(`/goals/${learned.routines_goal}`);
    await until('routines table', () => text().includes('Swim laps') && text().includes('Weekly'), 30000);
    await sleep(1000);
    await mark('shot 08-routines-goal');

    await go('/settings/location/log');
    await until('log', () => text().includes('Iron Works') || text().includes('City pool'), 30000);
    await mark('shot 09-location-log');

    await go('/settings/location');
    await until('settings', () => text().includes('Clear visit history'));
    show('Clear visit history');
    button('Clear visit history').click();
    await sleep(500);
    button('Delete visits').click();
    await sleep(2500);
    const cleared = await view();
    check(cleared.settings.visits === 0 && cleared.log.length === 0, 'history not cleared');
    check(cleared.places.length === 2 && cleared.routines.length === 1, 'clearing removed places or routines');
    show('Clear visit history');
    await mark('shot 10-cleared-history');
    await invoke('app_dev_mark', { mark: 'done' });
  } catch (e) {
    await fail(e);
  }
})();
