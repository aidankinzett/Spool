<script lang="ts">
  import '../app.css';
  import ToastStack from '$lib/components/ToastStack.svelte';

  let { children } = $props();
</script>

{@render children()}

<!-- Global toast stack — overlaid on every route. -->
<ToastStack />
