import { invoke } from '@tauri-apps/api/core';

/** One plugin version of a goal: `tree` is the validated tree, or null with the validator's `error`. */
export type PluginVersion = { id: string; name: string; version: number; enabled: boolean; active: boolean; tree: unknown | null; error: string | null; created_run_id?: string | null };

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

/** One live plugin as the coach library lists it: bundle title, version, whether it is its goal's active one, and its goal (none for a library plugin). */
export type PluginSummary = { id: string; name: string; title: string; version: number; active: boolean; goal_id: string | null; goal_title: string | null };

/** A tracker check-in cadence: daily, or weekly on `weekdays` (`Mon` to `Sun`), at local `at` (`HH:MM`). */
export type PluginCadence = { freq: 'daily' | 'weekly'; weekdays?: string[]; at: string };

/** One tracker field a bundle declares: key, label, kind (`number`, `text`, `bool`, `choice`) and an optional unit. */
export type PluginField = { key: string; label: string; kind: string; unit?: string | null };

/** One tracker a bundle declares, with its fields and its check-in cadence, if any. */
export type PluginTracker = { name: string; fields: PluginField[]; cadence: PluginCadence | null };

/** One plugin version for its read-only page: the summary, the raw bundle, its tree and trackers, or the validator's error. */
export type PluginDetail = PluginSummary & { bundle_toml: string; tree: unknown | null; error: string | null; trackers: PluginTracker[] };

/** Every live plugin across goals, one per goal and name: the active version, else the newest. */
export const allPlugins = () => invoke<PluginSummary[]>('plugins_all');

/** One live plugin for its read-only page; rejects when it was deleted or its goal is gone. */
export const getPlugin = (pluginId: string) => invoke<PluginDetail>('plugins_get', { pluginId });
