import CalendarDays from '@lucide/svelte/icons/calendar-days';
import House from '@lucide/svelte/icons/house';
import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
import BookOpenText from '@lucide/svelte/icons/book-open-text';
import NotebookPen from '@lucide/svelte/icons/notebook-pen';
import Target from '@lucide/svelte/icons/target';

/** A tab of the bottom bar. Inbox and Settings are reached from the top bar's bell and gear (Q69). */
export type TabId = 'today' | 'calendar' | 'goals' | 'notes' | 'dashboard' | 'log';

/** Every top-level destination: the six tabs plus the bell's Inbox and the gear's Settings. */
export type Place = TabId | 'inbox' | 'settings';

/** The six main tabs in display order, shared by the phone tab bar and the desktop sidebar. */
export const tabs = [
	{ id: 'today', label: 'Today', icon: House },
	{ id: 'calendar', label: 'Calendar', icon: CalendarDays },
	{ id: 'goals', label: 'Goals', icon: Target },
	{ id: 'notes', label: 'Notes', icon: BookOpenText },
	{ id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
	{ id: 'log', label: 'Log', icon: NotebookPen }
] as const satisfies readonly { id: TabId; label: string; icon: unknown }[];
