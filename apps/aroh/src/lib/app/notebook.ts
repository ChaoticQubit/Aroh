import { invoke } from '@tauri-apps/api/core';
import type { Block } from './blocks';

/** What a page belongs to: a free page, a day, an action or a goal. */
export type PageKind = 'page' | 'day' | 'action' | 'goal';

/** A page as the store opens it: `target` is the date, action id or goal id, or the page's own id for a free page. */
export type PageView = {
	id: string;
	kind: PageKind;
	target: string;
	title: string;
	parent_id: string | null;
	blocks: Block[];
	children: { id: string; title: string }[];
	backlinks: { id: string; kind: PageKind; title: string }[];
	updated_at: string;
};

/** One row of the page tree; `updated_at` is RFC 3339 UTC. */
export type TreeRow = { id: string; kind: PageKind; title: string; parent_id: string | null; updated_at: string };

/** One page search hit with a text snippet. */
export type SearchHit = { id: string; kind: PageKind; title: string; snippet: string };

/** Opens a page by kind and target; day, action and goal pages are created on first open. Kind 'page' opens any page by id. */
export const pageGet = (kind: PageKind, target: string) => invoke<PageView>('app_page_get', { kind, target });

/** Creates a free page under `parent_id` (null for the top level). */
export const pageCreate = (parent_id: string | null, title: string) => invoke<PageView>('app_page_create', { parent_id, title });

/** Saves the full ordered block list of a page. */
export const pageSave = (page_id: string, blocks: Block[]) => invoke<PageView>('app_page_save', { page_id, blocks });

/** Renames a page. */
export const pageRename = (page_id: string, title: string) => invoke<PageView>('app_page_rename', { page_id, title });

/** Moves a page under `parent_id` (null for the top level). */
export const pageMove = (page_id: string, parent_id: string | null) => invoke<PageView>('app_page_move', { page_id, parent_id });

/** Deletes a page. */
export const pageDelete = (page_id: string) => invoke<null>('app_page_delete', { page_id });

/** Every page as a flat tree row list. */
export const pagesTree = () => invoke<TreeRow[]>('app_pages_tree');

/** Pages whose title or text matches `query`. */
export const pagesSearch = (query: string) => invoke<SearchHit[]>('app_pages_search', { query });
