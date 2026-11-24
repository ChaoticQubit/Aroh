import * as client from './client';
import { catalog } from './catalog';
import { engine } from './engine';

/** The vendor's provider row, created with its first model when the owner never picked it. */
async function providerRow(providers: client.ProviderSetting[], vendor: string): Promise<client.ProviderSetting | null> {
	const row = providers.find((p) => p.vendor === vendor);
	if (row) return row;
	const model = catalog.find((c) => c.id === vendor)?.models[0];
	return model ? client.saveProvider(vendor, model) : null;
}

/**
 * Hands a typed key (if any) to the engine and runs its one-token test for `vendor`; the app
 * keeps no key. Used by onboarding and the Claude key page. Resolves `ok` with the engine's
 * status line, or not ok with the reason in the owner's words.
 */
export async function saveAndTestKey(providers: client.ProviderSetting[], vendor: string, key: string): Promise<{ ok: boolean; text: string }> {
	const row = await providerRow(providers, vendor).catch(() => null);
	if (!row) return { ok: false, text: 'Pick a provider first.' };
	if (key) {
		const saved = await engine.saveKey(vendor, key);
		if (!saved.ok) return { ok: false, text: saved.message };
	}
	const tested = await engine.testConnection(row.id);
	if (!tested.ok) return { ok: false, text: tested.message };
	return { ok: tested.value.startsWith('Connected'), text: tested.value };
}
