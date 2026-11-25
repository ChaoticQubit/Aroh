<script lang="ts">
	import icon from './aroh-icon.svg';

	/**
	 * The Aroh "Ring of Notes" mark on its dark tile (Q84), drawn from the one source SVG that also
	 * makes the app icons and the favicon. The tile stays dark in both themes. `size` is in CSS px;
	 * `label` names the image for assistive tech, and an empty label marks it decorative.
	 */
	let { size = 32, label = 'Aroh', class: className = '' }: { size?: number; label?: string; class?: string } = $props();
</script>

<img src={icon} alt={label} aria-hidden={label ? undefined : 'true'} width={size} height={size} draggable="false" class="shrink-0 rounded-[22%] select-none {className}" />
