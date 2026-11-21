import { addPluginListener, invoke, type PluginListener } from '@tauri-apps/api/core';
import type { KindTotal, TrackerEntry } from './types';

/** The device's offset from UTC in minutes, east positive, as `commands/intake.rs` expects. */
export function offsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}

/** Daily check-in settings as `intake_get_checkin` returns them; `time` is local `HH:MM`. */
export interface Checkin {
  enabled: boolean;
  time: string;
}

/** Daily targets; a null field leaves that target unchanged. */
export interface Targets {
  water_ml: number | null;
  workout_min: number | null;
  food_kcal: number | null;
}

/** Typed wrappers over the trackers commands in `commands/intake.rs`. */
export const intake = {
  logText: (text: string) => invoke<TrackerEntry[]>('intake_log_text', { text }),
  logChip: (chip: string) => invoke<TrackerEntry[]>('intake_log_chip', { chip }),
  listToday: () => invoke<TrackerEntry[]>('intake_list_today', { offsetMinutes: offsetMinutes() }),
  totals: () => invoke<KindTotal[]>('intake_totals', { offsetMinutes: offsetMinutes() }),
  setTargets: (targets: Targets) => invoke<void>('intake_set_targets', { targets }),
  getCheckin: () => invoke<Checkin>('intake_get_checkin'),
  setCheckin: (enabled: boolean, time: string) => invoke<Checkin>('intake_set_checkin', { enabled, time }),
  checkinQuestion: () => invoke<string>('intake_checkin_question', { offsetMinutes: offsetMinutes() }),
};

/**
 * Calls `open` with the route stored in a tapped notification's `extra.route`.
 * Only same-app paths starting with `/` are accepted. Returns the listener to unregister.
 */
export async function onNotificationRoute(open: (route: string) => void): Promise<PluginListener | null> {
  return addPluginListener<{ notification?: { extra?: Record<string, unknown> } }>(
    'notification',
    'actionPerformed',
    (event) => {
      const route = event.notification?.extra?.route;
      if (typeof route === 'string' && route.startsWith('/') && !route.startsWith('//')) open(route);
    },
  ).catch(() => null);
}
