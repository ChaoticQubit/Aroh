import type { GoalFile } from './client';
import type { Attachment } from '$lib/ui/components/goal/Attachments.svelte';

/** A byte count in decimal units: `300 B`, `3 KB`, `1.3 MB`. */
function size(bytes: number): string {
	console.assert(bytes >= 0, 'sizes are never negative');
	if (bytes < 1000) return `${bytes} B`;
	if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} KB`;
	return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

/** The attachment kind of a file of type `mime` named `name`. */
function kindOf(mime: string, name: string, image: boolean): Attachment['kind'] {
	if (image || mime.startsWith('image/')) return 'photo';
	if (mime === 'application/pdf' || name.toLowerCase().endsWith('.pdf')) return 'pdf';
	return 'file';
}

const LABEL: Record<Attachment['kind'], string> = { photo: 'Photo', pdf: 'PDF', file: 'File', link: 'Link' };

/** A stored goal file as the attachment list shows it, saying what the agents get from it. */
export function attachmentOf(f: GoalFile): Attachment {
	if (f.kind === 'link') return { id: f.id, kind: 'link', name: f.name, detail: 'Link · agents read it', failed: false };
	const kind = kindOf(f.mime ?? '', f.name, f.kind === 'image');
	const failed = f.ingest_status === 'failed';
	const read = kind === 'photo' ? 'agents see the name only' : failed ? 'Could not read text' : 'text read';
	return { id: f.id, kind, name: f.name, detail: `${LABEL[kind]} · ${size(f.size_bytes)} · ${read}`, failed };
}

/** Goal `goalId`'s attachments, oldest first (ids are time ordered). */
export function goalAttachments(files: GoalFile[], goalId: string): Attachment[] {
	return files
		.filter((f) => f.goal_id === goalId)
		.sort((a, b) => a.id.localeCompare(b.id))
		.map(attachmentOf);
}

/** Attachment `a` after its upload failed: kept with its file so the owner can retry. */
export function uploadFailed(a: Attachment): Attachment {
	return { ...a, detail: 'Upload failed', failed: true, retry: true };
}

/** A file picked on the New Goal form, shown until Create uploads it. */
export function pendingAttachment(id: string, file: File): Attachment {
	const kind = kindOf(file.type, file.name, false);
	return { id, kind, name: file.name, detail: `${LABEL[kind]} · ${size(file.size)} · uploads on Create`, failed: false };
}
