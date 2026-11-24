import { daysBetween, formatDate } from '$lib/ui/format';

/** The numbers the server adds to the limits card (Q64); any can be null. Dates are "YYYY-MM-DD". */
export type LimitsCard = { cap: number | null; today: string; target: string | null; finish: string | null; needed: number | null; raise_to: number | null; fits_minutes: number | null };

/** One choice on the card: `value` is the server's choice text, sent back unchanged; `minutes` prefills a raise. */
export type LimitChoice = { value: string; kind: 'move' | 'raise' | 'lower' | 'other'; title: string; detail: string; minutes?: number };

/** What the can't-fit card shows: the headline, the today-target-finish bar when dates are known, and the choices. */
export type LimitsView = { headline: string; bar: { today: string; target: string; finish: string; at: number; pace: string } | null; choices: LimitChoice[] };

const day = (ymd: string) => formatDate(ymd, { day: 'numeric', month: 'short' });

/**
 * Builds the can't-fit card from the limit_choice question text, its `choices` and the server's
 * `card` numbers (null on an older server). The question tail "What should change?" is dropped;
 * the bar and per-choice numbers appear only when the numbers exist.
 */
export function limitsView(text: string, choices: string[], card: LimitsCard | null): LimitsView {
	const headline = text.replace(/\s*What should change\?\s*$/, '');
	const span = card?.target && card.finish ? daysBetween(card.today, card.finish) : 0;
	const bar =
		card?.target && card.finish && span > 0
			? { today: day(card.today), target: day(card.target), finish: day(card.finish), at: Math.round((daysBetween(card.today, card.target) / span) * 1000) / 10, pace: card.cap ? `At ${card.cap} min` : 'At this pace' }
			: null;
	const cap = card?.cap ? `${card.cap} min a day` : 'your minutes a day';
	return {
		headline,
		bar,
		choices: choices.map((value): LimitChoice => {
			const word = value.toLowerCase();
			if (word.startsWith('move')) return { value, kind: 'move', title: value, detail: `Keep ${cap}` };
			if (word.startsWith('raise')) {
				return card?.raise_to && card.target
					? { value, kind: 'raise', title: `Raise to ${card.raise_to} min a day`, detail: `Finishes by ${day(card.target)}`, minutes: card.raise_to }
					: { value, kind: 'raise', title: value, detail: 'Pick your new minutes a day' };
			}
			if (word.startsWith('lower')) {
				const share = card?.fits_minutes && card.needed && card.target ? Math.round((card.fits_minutes / card.needed) * 100) : null;
				return { value, kind: 'lower', title: value, detail: share !== null && card?.target ? `About ${share}% fits by ${day(card.target)} at ${cap}` : 'Keep your minutes and your date' };
			}
			return { value, kind: 'other', title: value, detail: '' };
		})
	};
}
