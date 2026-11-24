import { describe, expect, it } from 'vitest';
import { backTarget, nextStack, parentOf, screenTitle } from './back';

describe('screenTitle', () => {
	it('names tab roots, sub-screens and goal screens', () => {
		expect(screenTitle('/today')).toBe('Today');
		expect(screenTitle('/dashboard')).toBe('Dashboard');
		expect(screenTitle('/goals/abc')).toBe('Goal');
		expect(screenTitle('/goals/new')).toBe('New goal');
		expect(screenTitle('/goals/abc/chat')).toBe('Goal chat');
		expect(screenTitle('/actions/abc')).toBe('Session');
		expect(screenTitle('/actions/abc/chat')).toBe('Card chat');
		expect(screenTitle('/settings/location/log')).toBe('Location log');
		expect(screenTitle('/personas?goal=abc')).toBe('Coaches');
		expect(screenTitle('/inbox')).toBe('Inbox');
	});
});

describe('parentOf', () => {
	it('has no parent on tab roots, Settings and Inbox', () => {
		for (const p of ['/today', '/calendar', '/goals', '/dashboard', '/log', '/settings', '/inbox']) expect(parentOf(p)).toBeNull();
	});

	it('gives the screen above each sub-screen', () => {
		expect(parentOf('/goals/abc')).toEqual({ path: '/goals', label: 'Goals' });
		expect(parentOf('/goals/new')).toEqual({ path: '/goals', label: 'Goals' });
		expect(parentOf('/goals/abc/chat')).toEqual({ path: '/goals/abc', label: 'Goal' });
		expect(parentOf('/actions/abc/chat')).toEqual({ path: '/actions/abc', label: 'Session' });
		expect(parentOf('/actions/abc')).toEqual({ path: '/today', label: 'Today' });
		expect(parentOf('/settings/places')).toEqual({ path: '/settings', label: 'Settings' });
		expect(parentOf('/settings/location/log')).toEqual({ path: '/settings/location', label: 'Location' });
		expect(parentOf('/personas?goal=abc')).toEqual({ path: '/goals/abc', label: 'Goal' });
		expect(parentOf('/personas')).toEqual({ path: '/settings', label: 'Settings' });
	});
});

describe('nextStack', () => {
	it('starts on entry, pushes on links and pops on browser back', () => {
		let s = nextStack([], '/today', 'enter');
		s = nextStack(s, '/goals/abc', 'goto');
		expect(s).toEqual(['/today', '/goals/abc']);
		s = nextStack(s, '/today', 'popstate');
		expect(s).toEqual(['/today']);
	});

	it('restarts from the screen when a popstate does not match the stack', () => {
		expect(nextStack(['/today', '/goals/abc'], '/log', 'popstate')).toEqual(['/log']);
	});

	it('ignores a navigation to the same screen', () => {
		expect(nextStack(['/today'], '/today', 'goto')).toEqual(['/today']);
	});
});

describe('backTarget', () => {
	it('labels back with the previous screen and goes back in history', () => {
		expect(backTarget(['/today', '/goals/abc'])).toEqual({ label: 'Today', history: true, path: '/today' });
	});

	it('falls back to the parent on a cold start into a sub-screen', () => {
		expect(backTarget(['/goals/abc'])).toEqual({ label: 'Goals', history: false, path: '/goals' });
	});

	it('shows no back on a tab root, even with history', () => {
		expect(backTarget(['/goals/abc', '/goals'])).toBeNull();
	});
});
