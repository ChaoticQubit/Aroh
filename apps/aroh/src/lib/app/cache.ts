import type { Call } from './engine';

/** The part of `Storage` the engine cache persists in; tests pass a map. */
export type CacheStore = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

/** Most engine reads kept; the oldest written drops first. */
export const MAX_ENTRIES = 200;

/** Most bytes (JSON characters) of kept reads; the oldest written drops first. */
export const MAX_BYTES = 512_000;

const STORAGE_KEY = 'aroh.engine-cache.v1';
const ID = '[0-9A-Za-z_-]+';
const STATE = 'state=[a-z_]+';

/**
 * The engine GET paths whose answers may be kept (B87): runs lists, a run view without
 * `after_seq`, personas, the run limit, insights and the account (its consent only). Nothing
 * else, and never a path that mentions keys.
 */
const ALLOWED = [
	new RegExp(`^/v1/runs(\\?(goal_id=${ID}(&${STATE})?|${STATE}))?$`),
	new RegExp(`^/v1/runs/${ID}$`),
	/^\/v1\/personas$/,
	/^\/v1\/settings\/run-limit$/,
	new RegExp(`^/v1/insights(\\?goal_id=${ID})?$`),
	/^\/v1\/account$/
];

/** Whether the answer of `GET path` may be kept. Anything key-related never is. */
export function cachedPath(path: string): boolean {
	if (/key/i.test(path)) return false;
	return ALLOWED.some((r) => r.test(path));
}

/** What is kept of an answer: the account read keeps its consent time only, the rest whole. */
function kept(path: string, body: unknown): unknown {
	if (path !== '/v1/account') return body;
	return { ai_consent_at: (body as { ai_consent_at?: string | null } | null)?.ai_consent_at ?? null };
}

/** The resource a `/v1/<resource>/...` path belongs to. */
const resourceOf = (path: string): string => path.split('?')[0].split('/')[2] ?? '';

/** What the cache holds on disk: the account it belongs to and its reads, oldest first. */
type Saved = { account: string | null; entries: [string, unknown][] };

/**
 * Persisted stale-while-revalidate store for engine reads (B87), in web storage so a page reads
 * it synchronously in its first frame. Keyed by account id and path; it serves nothing until the
 * signed-in account is confirmed this session (`owner`), and is cleared on sign-out and whenever
 * the account changes. Every storage access is guarded, so a missing or blocked store only means
 * nothing persists.
 */
export class EngineCache {
	private account: string | null = null;
	private confirmed = false;
	private generation = 0;
	private entries = new Map<string, { body: unknown; bytes: number }>();
	private bytes = 0;

	constructor(private store: CacheStore | null) {
		const saved = this.load();
		if (!saved) return;
		this.account = saved.account;
		for (const [path, body] of saved.entries) this.keep(path, body);
	}

	/** Sets the signed-in account (null: signed out). A different account than the cache's clears it. */
	owner(account: string | null) {
		if (account !== this.account) {
			this.entries.clear();
			this.bytes = 0;
			this.account = account;
			this.generation++;
			this.save();
		}
		this.confirmed = true;
	}

	/** The kept answer of `GET path` for the confirmed account, or undefined. */
	get<T>(path: string): T | undefined {
		if (!this.confirmed || !this.account || !cachedPath(path)) return undefined;
		return this.entries.get(path)?.body as T | undefined;
	}

	/**
	 * Keeps the answer of `GET path` read under `ticket` (`current()` when the read started); a read
	 * started before a sign-out or an account change is dropped.
	 */
	put(ticket: string | null, path: string, body: unknown) {
		if (!ticket || ticket !== this.current() || !cachedPath(path)) return;
		if (!this.keep(path, kept(path, body))) return;
		this.save();
	}

	/** Drops the reads a successful `method path` write changed; deleting the account drops everything. */
	invalidate(method: string, path: string) {
		const resource = resourceOf(path);
		if (method === 'DELETE' && path === '/v1/account') return this.owner(null);
		const touched = resource === 'personas' || resource === 'settings' || resource === 'account' ? [resource] : resource === 'keys' ? [] : ['runs', 'insights'];
		let dropped = false;
		for (const p of [...this.entries.keys()]) {
			if (!touched.includes(resourceOf(p))) continue;
			this.bytes -= this.entries.get(p)?.bytes ?? 0;
			this.entries.delete(p);
			dropped = true;
		}
		if (dropped) this.save();
	}

	/** The ticket of a read starting now: account and generation, null when the cache serves none. */
	current(): string | null {
		return this.confirmed && this.account ? `${this.generation}:${this.account}` : null;
	}

	/** How many reads and bytes are kept. */
	size(): { entries: number; bytes: number } {
		return { entries: this.entries.size, bytes: this.bytes };
	}

	/** Adds `path` as the newest read, then drops the oldest past the caps; false when it alone is over the byte cap. */
	private keep(path: string, body: unknown): boolean {
		const bytes = JSON.stringify(body ?? null).length;
		if (bytes > MAX_BYTES) return false;
		const old = this.entries.get(path);
		if (old) this.bytes -= old.bytes;
		this.entries.delete(path);
		this.entries.set(path, { body, bytes });
		this.bytes += bytes;
		for (const oldest of this.entries.keys()) {
			if (this.entries.size <= MAX_ENTRIES && this.bytes <= MAX_BYTES) break;
			this.bytes -= this.entries.get(oldest)?.bytes ?? 0;
			this.entries.delete(oldest);
		}
		console.assert(this.entries.size <= MAX_ENTRIES && this.bytes <= MAX_BYTES && this.bytes >= 0, 'the cache stays within its caps');
		return true;
	}

	/** Reads the saved cache, or null when there is none or it cannot be read. */
	private load(): Saved | null {
		try {
			const raw = this.store?.getItem(STORAGE_KEY);
			const saved = raw ? (JSON.parse(raw) as Saved) : null;
			return saved && Array.isArray(saved.entries) ? saved : null;
		} catch {
			return null;
		}
	}

	/** Writes the cache; a full or blocked store leaves it in memory only. */
	private save() {
		try {
			if (!this.account || !this.entries.size) return this.store?.removeItem(STORAGE_KEY);
			const saved: Saved = { account: this.account, entries: [...this.entries].map(([p, e]) => [p, e.body]) };
			this.store?.setItem(STORAGE_KEY, JSON.stringify(saved));
		} catch {
			return;
		}
	}
}

/**
 * Wraps an engine transport so every successful allowlisted GET is kept in `cache` for the
 * account that was current when it started, and every successful write drops what it changed.
 */
export function caching(call: Call, cache: EngineCache): Call {
	return async (method, path, body) => {
		const ticket = cache.current();
		const answer = await call(method, path, body);
		const ok = answer.status >= 200 && answer.status < 300;
		if (method === 'GET' && answer.status === 200) cache.put(ticket, path, answer.body);
		if (method !== 'GET' && ok) cache.invalidate(method, path);
		return answer;
	};
}

/** This device's web storage, or null where there is none or it is blocked. */
function deviceStore(): CacheStore | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}

/** The engine cache the app uses. */
export const engineCache = new EngineCache(deviceStore());
