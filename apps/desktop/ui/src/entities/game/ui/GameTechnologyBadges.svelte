<script lang="ts">
  import { Badge, BadgeGroup } from '@shared/ui';

  type Props = {
    technologies?: string[];
  };

  let { technologies = [] }: Props = $props();
</script>

<BadgeGroup class="technology-row">
  {#if technologies.length === 0}
    <Badge pill surface="outline" tone="muted">No detected technologies yet</Badge>
  {:else}
    {#each technologies as technology (technology)}
      <Badge pill surface="outline">{technology}</Badge>
    {/each}
  {/if}
</BadgeGroup>
