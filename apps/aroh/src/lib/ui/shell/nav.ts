import CalendarDays from '@lucide/svelte/icons/calendar-days';
import House from '@lucide/svelte/icons/house';
import Inbox from '@lucide/svelte/icons/inbox';
import NotebookPen from '@lucide/svelte/icons/notebook-pen';
import Target from '@lucide/svelte/icons/target';

/** A top-level destination. Settings is reached from the avatar, not a tab. */
export type TabId = 'today' | 'calendar' | 'goals' | 'inbox' | 'log';

/** The five main tabs in display order, shared by the phone tab bar and the desktop sidebar. */
export const tabs = [
	{ id: 'today', label: 'Today', icon: House },
	{ id: 'calendar', label: 'Calendar', icon: CalendarDays },
	{ id: 'goals', label: 'Goals', icon: Target },
	{ id: 'inbox', label: 'Inbox', icon: Inbox },
	{ id: 'log', label: 'Log', icon: NotebookPen }
] as const satisfies readonly { id: TabId; label: string; icon: unknown }[];
