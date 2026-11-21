import { invoke } from '@tauri-apps/api/core';
import type { StoredReplan } from './types';

/** Event names the replan commands emit, each with a StoredReplan payload. */
export const REPLAN_APPLIED = 'replan-applied';
export const REPLAN_PROPOSED = 'replan-proposed';

/** Marks overdue actions missed and replans them; returns the newly missed action ids. */
export const checkMissed = () => invoke<string[]>('check_missed');

/** Lists replans waiting for review, newest first. */
export const listProposed = () => invoke<StoredReplan[]>('replan_list_proposed');

/** Applies a proposed replan; fails when the calendar changed since it was made. */
export const applyReplan = (id: string) => invoke<void>('replan_apply', { id });

/** Rejects a proposed replan. */
export const rejectReplan = (id: string) => invoke<void>('replan_reject', { id });

/** Undoes an applied replan. */
export const undoReplan = (id: string) => invoke<void>('replan_undo', { id });
