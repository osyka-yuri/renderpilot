<script lang="ts">
  import Badge from '@shared/ui/Badge.svelte';
  import BadgeGroup from '@shared/ui/BadgeGroup.svelte';

  type Props = {
    technologies?: string[];
  };

  let { technologies = [] }: Props = $props();
</script>

<BadgeGroup class="technology-row">
  {#if technologies.length === 0}
    <Badge pill surface="outline" tone="muted">No detected technologies yet</Badge>
  {:else}
    {#each technologies as technology}
      <Badge pill surface="outline">{technology}</Badge>
    {/each}
  {/if}
</BadgeGroup>
