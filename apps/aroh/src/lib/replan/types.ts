/** Time placement of an action, matching the Rust `Slot` JSON. Times are UTC ISO 8601. */
export interface Slot {
  start_at: string;
  end_at: string;
  est_minutes: number;
}

/** One replan diff entry, matching `replans.diff_json`. `goal_id` and `title` are set on create. */
export interface DiffOp {
  action_id: string;
  op: 'move' | 'create' | 'delete' | 'resize';
  before: Slot | null;
  after: Slot | null;
  goal_id?: string;
  title?: string;
}

/** Minutes of a goal's missed work that found no room before its deadline. */
export interface Infeasible {
  goal_id: string;
  unplaced_minutes: number;
}

/** Planner output: the diff to apply and what could not be placed. */
export interface Proposal {
  diff: DiffOp[];
  infeasible: Infeasible[];
}

/** Short local label for a slot, such as "Tue 24 Nov, 09:00-10:12 (72 min)". */
export function slotLabel(slot: Slot | null): string {
  if (!slot) return 'None';
  const start = new Date(slot.start_at);
  const end = new Date(slot.end_at);
  const day = start.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
  const time = (d: Date) => d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  return `${day}, ${time(start)}-${time(end)} (${slot.est_minutes} min)`;
}

/** One stored replan row as the replan commands return it. */
export interface StoredReplan {
  id: string;
  size: 'small' | 'big';
  status: 'proposed' | 'applied' | 'rejected' | 'undone';
  created_at: string;
  proposal: Proposal;
}
