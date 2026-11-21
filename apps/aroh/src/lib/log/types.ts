/** Tracker kind, matching `aroh_intake::TrackerKind` serialized lowercase. */
export type TrackerKind = 'food' | 'water' | 'workout' | 'other';

/** One `tracker_entries` row as `aroh_intake::TrackerEntry` serializes it; water in ml, workout in min. */
export interface TrackerEntry {
  id: string;
  updated_at: string;
  deleted: boolean;
  kind: TrackerKind;
  amount: number;
  unit: string;
  note: string | null;
  raw_text: string;
  logged_at: string;
}

/** One day's total for a kind, matching `aroh_intake::KindTotal`; `target` is null when unset. */
export interface KindTotal {
  kind: TrackerKind;
  unit: string;
  total: number;
  target: number | null;
  remaining: number | null;
}

/** A quick-tap chip: the label shown and the chip text sent to `parse_chip`. */
export interface Chip {
  label: string;
  chip: string;
}

/** Default chips; each `chip` is a string `aroh_intake::parse_chip` accepts. */
export const DEFAULT_CHIPS: readonly Chip[] = [
  { label: 'Glass of water', chip: 'water' },
  { label: 'Water 500 ml', chip: 'water 500ml' },
  { label: 'Workout 30 min', chip: 'workout 30min' },
];

/** Human name for a kind, used in headings and labels. */
export const KIND_LABEL: Record<TrackerKind, string> = {
  food: 'Food',
  water: 'Water',
  workout: 'Workout',
  other: 'Other',
};
