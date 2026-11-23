import { describe, expect, it } from 'vitest';
import { attachmentOf, goalAttachments, pendingAttachment, uploadFailed } from './files';
import type { GoalFile } from './client';

const row = (over: Partial<GoalFile>): GoalFile => ({
	id: 'f1',
	goal_id: 'g1',
	name: 'Joy of X.pdf',
	kind: 'document',
	storage_path: 'goals/g1/f1-Joy-of-X.pdf',
	url: null,
	mime: 'application/pdf',
	size_bytes: 1_250_000,
	ingest_status: 'done',
	...over
});

describe('attachments', () => {
	it('names what the agents get from each file', () => {
		expect(attachmentOf(row({}))).toEqual({ id: 'f1', kind: 'pdf', name: 'Joy of X.pdf', detail: 'PDF · 1.3 MB · text read', failed: false });
		expect(attachmentOf(row({ ingest_status: 'failed' }))).toMatchObject({ detail: 'PDF · 1.3 MB · Could not read text', failed: true });
		expect(attachmentOf(row({ kind: 'image', mime: 'image/jpeg', name: 'shelf.jpg', size_bytes: 2048 }))).toMatchObject({ kind: 'photo', detail: 'Photo · 2 KB · agents see the name only' });
		expect(attachmentOf(row({ name: 'notes.md', mime: 'text/markdown', size_bytes: 300 }))).toMatchObject({ kind: 'file', detail: 'File · 300 B · text read' });
		expect(attachmentOf(row({ kind: 'link', storage_path: null, url: 'https://example.org/x', name: 'example.org/x', mime: null, size_bytes: 0, ingest_status: 'pending' }))).toMatchObject({
			kind: 'link',
			detail: 'Link · agents read it'
		});
	});

	it('lists a goal’s files oldest first and a picked file before upload', () => {
		const rows = [row({ id: 'f2', goal_id: 'g1' }), row({ id: 'f1', goal_id: 'g1' }), row({ id: 'f3', goal_id: 'g2' })];
		expect(goalAttachments(rows, 'g1').map((a) => a.id)).toEqual(['f1', 'f2']);
		const picked = pendingAttachment('p0', new File(['x'.repeat(3000)], 'scan.pdf', { type: 'application/pdf' }));
		expect(picked).toEqual({ id: 'p0', kind: 'pdf', name: 'scan.pdf', detail: 'PDF · 3 KB · uploads on Create', failed: false });
		expect(uploadFailed(picked)).toEqual({ ...picked, detail: 'Upload failed', failed: true, retry: true });
	});
});
