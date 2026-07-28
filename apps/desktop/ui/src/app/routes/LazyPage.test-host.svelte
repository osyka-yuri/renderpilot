<script lang="ts">
  import type { LazyPageResource } from './lazy-page-resource.svelte';
  import LazyPage from './LazyPage.svelte';
  import type TestPage from './LazyPage.test-page.svelte';

  type Props = {
    page: LazyPageResource<typeof TestPage>;
    onBack: () => void;
  };

  const { page, onBack }: Props = $props();
</script>

<LazyPage {page} {onBack}>
  {#snippet children(Page)}
    <Page label="Loaded page content" />
  {/snippet}
</LazyPage>
