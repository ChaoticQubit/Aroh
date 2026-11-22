(async () => {
  const { invoke, sleep, mark, until, go, text, button, fail } = window.AROH_LIB;
  const scroller = () => [...document.querySelectorAll('*')].find((el) => el.scrollHeight > el.clientHeight + 40 && getComputedStyle(el).overflowY.match(/auto|scroll/)) ?? document.scrollingElement;
  const scrollTo = async (heading) => {
    const h = [...document.querySelectorAll('h3, p')].find((x) => x.textContent.trim() === heading);
    if (!h) throw new Error(`no heading ${heading}`);
    const box = scroller();
    box.scrollTop += h.getBoundingClientRect().top - 130;
    await sleep(800);
  };
  const top = async () => {
    scroller().scrollTop = 0;
    await sleep(600);
  };
  const tap = async (name) => {
    button(name).click();
    await sleep(1500);
  };
  const v2 = `aroh_plugin = 1
name = "fitness"
title = "Fitness, daily focus"

[[trackers]]
name = "Weight"

[[trackers.fields]]
key = "weight"
label = "Weight"
kind = "number"
unit = "kg"
target = 78

[[trackers]]
name = "Protein"

[[trackers.fields]]
key = "protein"
label = "Protein"
kind = "number"
unit = "g"
target = 150

[[sections]]
title = "Today"

[[sections.items]]
kind = "stat_tile"
label = "Weight now"
unit = "kg"
value = { bind = "latest", tracker = "Weight", field = "weight" }
delta = { bind = "change", tracker = "Weight", field = "weight" }

[[sections.items]]
kind = "target_vs_actual"
label = "Weight goal"
unit = "kg"
target = { bind = "target", tracker = "Weight", field = "weight" }
actual = { bind = "latest", tracker = "Weight", field = "weight" }

[[sections.items]]
kind = "week_strip"
label = "This week"
actions = { bind = "actions", when = "week" }
startDay = { bind = "week_start" }

[[sections]]
title = "Nutrition"

[[sections.items]]
kind = "bar_chart"
label = "Protein per day"
unit = "g"
points = { bind = "series", tracker = "Protein", field = "protein" }
target = { bind = "target", tracker = "Protein", field = "protein" }
series = 3
`;
  try {
    await until('the app shell', () => text().includes('Today'));
    await invoke('app_dev_seed');
    const now = Date.now();
    const s = await invoke('app_snapshot', { from: new Date(now - 30 * 864e5).toISOString(), to: new Date(now + 30 * 864e5).toISOString() });
    const six = s.goals.find((g) => g.title === 'Six pack by Feb 1');
    const piano = s.goals.find((g) => g.title === 'Learn piano');
    let versions = await invoke('plugins_list', { goalId: six.id });
    if (!versions.length) {
      await invoke('plugins_use_template', { goalId: six.id, name: 'fitness' });
      versions = await invoke('plugins_save', { goalId: six.id, bundleToml: v2 });
    }
    const v1 = versions.find((v) => v.version === 1);
    await invoke('plugins_enable', { goalId: six.id, pluginId: v1.id });
    await go(`/goals/${six.id}`);
    await until('the v1 dashboard', () => text().includes('Last 4 weeks') && text().includes('Dashboard'));
    await top();
    await mark('shot 01-plugin-v1-top');
    await scrollTo('Progress');
    await mark('shot 02-plugin-v1-progress');
    await scrollTo('Upcoming');
    await mark('shot 03-plugin-v1-upcoming');
    await top();
    await tap('v2');
    await until('the v2 dashboard', () => text().includes('Nutrition') && text().includes('Weight goal'));
    await mark('shot 04-plugin-v2-switched');
    await scrollTo('Today');
    await mark('shot 05-plugin-v2-today');
    await go(`/goals/${piano.id}`);
    await top();
    await until('the derived dashboard', () => text().includes('Learn piano') && !text().includes('Pick a plugin version'));
    await mark('shot 06-no-plugin-derived');
    await mark('done');
  } catch (e) {
    await fail(e);
  }
})();
