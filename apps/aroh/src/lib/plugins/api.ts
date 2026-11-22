import { invoke } from '@tauri-apps/api/core';

/** One plugin version of a goal: `tree` is the validated tree, or null with the validator's `error`. */
export type PluginVersion = { id: string; name: string; version: number; enabled: boolean; active: boolean; tree: unknown | null; error: string | null };

/** The goal's plugin versions, newest first. */
export const listPlugins = (goalId: string) => invoke<PluginVersion[]>('plugins_list', { goalId });

/** Makes one version the goal's dashboard, or with null shows the derived dashboard. */
export const enablePlugin = (goalId: string, pluginId: string | null) => invoke<PluginVersion[]>('plugins_enable', { goalId, pluginId });

/** Stores an edited bundle as the goal's next enabled version; errors name the file and field. */
export const savePlugin = (goalId: string, bundleToml: string) => invoke<PluginVersion[]>('plugins_save', { goalId, bundleToml });

/** Stores a built-in template as the goal's next enabled version. */
export const useTemplate = (goalId: string, name: string) => invoke<PluginVersion[]>('plugins_use_template', { goalId, name });

/** The version the dashboard renders: the active one, only when it validated. */
export function activeVersion(versions: PluginVersion[]): PluginVersion | null {
	return versions.find((v) => v.active && v.tree !== null) ?? null;
}
