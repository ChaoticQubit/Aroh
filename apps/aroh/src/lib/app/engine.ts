import type { ChatMessage } from '$lib/ui/contract';

/** Shown wherever an action needs the server engine (Q3): planning, chat, keys and tests. */
export const NEEDS_ENGINE = 'This needs a connection to the engine, which is not set up yet. Everything else works offline.';

/** The outcome of an engine call: a value, or the reason it could not run. */
export type EngineResult<T> = { ok: true; value: T } | { ok: false; reason: 'needs_engine'; message: string };

/**
 * The AI side of the app, served by the server engine (pod 9). The app holds no provider keys:
 * `saveKey` hands the key to the engine and keeps nothing. Every method resolves, never throws.
 */
export interface Engine {
	readonly connected: boolean;
	startGoal(text: string): Promise<EngineResult<ChatMessage[]>>;
	sendChat(goalId: string, text: string): Promise<EngineResult<ChatMessage[]>>;
	saveKey(providerId: string, key: string): Promise<EngineResult<null>>;
	testConnection(providerId: string): Promise<EngineResult<string>>;
}

/** Resolves every call to `needs_engine` without touching its arguments. */
function offline<T>(): Promise<EngineResult<T>> {
	return Promise.resolve({ ok: false, reason: 'needs_engine', message: NEEDS_ENGINE });
}

/** The engine client used until pod 9 ships: not connected, every call needs the engine. */
export const notConfigured: Engine = {
	connected: false,
	startGoal: () => offline(),
	sendChat: () => offline(),
	saveKey: () => offline(),
	testConnection: () => offline()
};

/** The engine client the app uses. */
export const engine: Engine = notConfigured;
