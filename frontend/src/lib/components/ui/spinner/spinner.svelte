<script lang="ts">
	import { cn } from "$lib/utils.js";
	import Loader2Icon from '@lucide/svelte/icons/loader-2';

	let {
		class: className,
		role = "status",
		"aria-label": ariaLabel = "Loading",
	}: { class?: string; role?: string; "aria-label"?: string } = $props();
</script>

<Loader2Icon {role} aria-label={ariaLabel} class={cn("size-4 animate-spin", className)} />
