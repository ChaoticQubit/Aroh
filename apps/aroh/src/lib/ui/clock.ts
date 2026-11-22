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

let storeZone: string | null = null;

/**
 * Records the IANA zone the store's rows were last shifted to (the snapshot's `zone`), so screens
 * and engine requests use it even while the web view still reports the old zone. Empty clears it.
 */
export function setDeviceZone(zone: string | null | undefined): void {
	storeZone = zone || null;
}

/** The zone screens and requests use: the store's zone when known, else the web view's. */
export function deviceZone(): string {
	return storeZone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/**
 * Reads the clock set by an ancestor, or the device clock and zone when none is set.
 * Must be called during component initialisation. Returns a Clock.
 */
export function getClock(): Clock {
	return getContext<Clock | undefined>(KEY) ?? { now: new Date().toISOString(), zone: deviceZone() };
}
