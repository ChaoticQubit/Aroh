/** A block's drawn box in one day column: top `y` and height `h`, in pixels. */
export type Box = { y: number; h: number };

/**
 * Lays out overlapping blocks side by side, like iOS Calendar. Input: drawn boxes of one day.
 * Output, in input order: each block's column `col` and its overlap cluster's column count
 * `cols`. Blocks that only touch do not overlap; a block takes the leftmost free column.
 */
export function overlapColumns(boxes: Box[]): { col: number; cols: number }[] {
	for (const b of boxes) if (!Number.isFinite(b.y) || !Number.isFinite(b.h) || b.h < 0) throw new RangeError(`bad box ${b.y} ${b.h}`);
	const out = boxes.map(() => ({ col: 0, cols: 1 }));
	const order = boxes.map((_, i) => i).sort((a, b) => boxes[a].y - boxes[b].y || boxes[b].h - boxes[a].h);
	let ends: number[] = [];
	let cluster: number[] = [];
	let clusterEnd = -Infinity;
	const close = () => cluster.forEach((i) => (out[i].cols = ends.length));
	for (const i of order) {
		const { y, h } = boxes[i];
		if (y >= clusterEnd) {
			close();
			ends = [];
			cluster = [];
			clusterEnd = -Infinity;
		}
		const free = ends.findIndex((end) => end <= y);
		const col = free < 0 ? ends.length : free;
		ends[col] = y + h;
		out[i].col = col;
		cluster.push(i);
		clusterEnd = Math.max(clusterEnd, y + h);
	}
	close();
	return out;
}
