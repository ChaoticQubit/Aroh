import { getContext, setContext } from 'svelte';

/** The instant and IANA zone components treat as "now" when they format or compare times. */
export type Clock = { now: string; zone: string };

const KEY = Symbol('aroh-clock');

/**
 * Provides a fixed clock to every component below the caller, for mockups and tests.
 * Must be called during component initialisation. Returns the clock it set.
 */
export function setClock(clock: Clock): Clock {
	return setContext(KEY, clock);
}

/**
 * Reads the clock set by an ancestor, or the device clock and zone when none is set.
 * Must be called during component initialisation. Returns a Clock.
 */
export function getClock(): Clock {
	return getContext<Clock | undefined>(KEY) ?? { now: new Date().toISOString(), zone: Intl.DateTimeFormat().resolvedOptions().timeZone };
}
