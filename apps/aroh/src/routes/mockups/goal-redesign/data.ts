import type { ProfileView } from '$lib/ui/screens/types';
import type { ActionView, CategorySlot } from '$lib/ui/contract';

/** One file, photo or link attached to a goal for the agents to read. */
export type Attachment = { id: string; kind: 'photo' | 'pdf' | 'file' | 'link'; name: string; detail: string };

/** Phase a goal card is shown in, one per mockup frame. */
export type Phase = 'researching' | 'questions' | 'stopped' | 'plan' | 'active';

/** One chat line inside the goal card. */
export type Line = { from: 'owner' | 'coach'; text: string };

/** The sample book goal every redesign frame is built around. */
export const book = {
	title: 'Finish reading Harbor Lights',
	description: 'I keep stopping halfway. I want to read a little every evening and finish before the new year.',
	target: '2026-12-31',
	category: 'Learning',
	slot: 1 as CategorySlot,
	coach: 'Reading coach'
};

/** Attachments on the filled New Goal form and on the goal card. */
export const attachments: Attachment[] = [
	{ id: 'a1', kind: 'pdf', name: 'harbor-lights-contents.pdf', detail: 'PDF, 2 pages, 180 KB' },
	{ id: 'a2', kind: 'photo', name: 'Bookmark page', detail: 'Photo, 1.2 MB' },
	{ id: 'a3', kind: 'link', name: 'Publisher page', detail: 'example.com/harbor-lights' }
];

/** Research steps while the run is still working. */
export const steps: { text: string; state: 'done' | 'running' | 'pending' }[] = [
	{ text: 'Read your PDF: table of contents found', state: 'done' },
	{ text: "Looking up the book's chapters and pages", state: 'running' },
	{ text: 'Check your calendars for free evenings', state: 'pending' },
	{ text: 'Draft a reading plan', state: 'pending' }
];

/** What research found, shown once it is done. */
export const found = [
	{ label: 'Chapters', value: '24' },
	{ label: 'Pages', value: '~320' },
	{ label: 'Reading time', value: '~9 h' },
	{ label: 'Free evenings', value: 'Mon to Fri after 20:00' }
];

/** Builds one planned reading session on the mock week. */
function session(id: string, day: string, start: string, end: string, title: string, status: ActionView['status'] = 'planned'): ActionView {
	return { id, title, start: `${day}T${start}:00Z`, end: `${day}T${end}:00Z`, status, locked: false, category: 1, goalTitle: book.title };
}

/** First sessions of the proposed plan, chapter 6 on. */
export const proposed: ActionView[] = [
	session('r1', '2026-11-26', '20:00', '20:30', 'Read chapters 6 and 7'),
	session('r2', '2026-11-27', '20:00', '20:30', 'Read chapter 8'),
	session('r3', '2026-11-29', '10:00', '10:30', 'Read chapters 9 and 10'),
	session('r4', '2026-11-30', '20:00', '20:30', 'Read chapter 11')
];

/** Chat lines per phase, all inside the goal card. */
export const chat: Record<Phase, Line[]> = {
	researching: [],
	questions: [{ from: 'coach', text: 'I found the book from your PDF. Three things I could not find are below.' }],
	stopped: [],
	plan: [
		{ from: 'coach', text: 'Here is a plan from chapter 6 at 30 minutes in the evening. Approve it, or tell me what to change.' }
	],
	active: [
		{ from: 'coach', text: 'Your plan is on the calendar. Tell me here if an evening gets busy.' },
		{ from: 'owner', text: 'I am away on Saturday.' },
		{ from: 'coach', text: 'Moved Saturday to Sunday 10:00. Nothing else changed. Undo in Inbox.' }
	]
};

/** Sample profile: facts from goal answers and preferences from edits, newest first, and one blocked topic. */
export const profile: ProfileView = {
	facts: [
		{ id: 'f1', text: 'Reading time of day: evening', source: 'From your edits', date: '2026-11-18T09:00:00Z', blockable: true },
		{ id: 'f2', text: 'Preferred format: audiobooks on runs', source: 'From your answer on Run a 10K', date: '2026-11-12T09:00:00Z', blockable: true },
		{ id: 'f3', text: 'No sessions before 07:00', source: 'From your edits', date: '2026-11-09T09:00:00Z', blockable: false },
		{ id: 'f4', text: 'Free minutes a day: 30 min', source: 'From your answer on Learn Spanish', date: '2026-11-03T09:00:00Z', blockable: true },
		{ id: 'f5', text: 'Gym days: Mon, Wed and Fri', source: 'From your answer on Six pack by Feb 1', date: '2026-11-01T09:00:00Z', blockable: true }
	],
	blocked: [{ id: 'b1', label: 'Body weight' }]
};
