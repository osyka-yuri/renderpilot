<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { Badge, BadgeGroup } from '@shared/ui';

  const EMPTY_LIBRARIES_LABEL = 'No detected libraries yet';

  type Props = HTMLAttributes<HTMLElement> & {
    libraries?: readonly string[];
  };

  let { libraries = [], class: className = '', ...rest }: Props = $props();

  const detectedLibraries = $derived(libraries.map((library) => library.trim()).filter(Boolean));
</script>

<BadgeGroup {...rest} class={className}>
  {#if detectedLibraries.length === 0}
    <Badge pill surface="outline" tone="muted">
      {EMPTY_LIBRARIES_LABEL}
    </Badge>
  {:else}
    {#each detectedLibraries as library (library)}
      <Badge pill surface="outline">
        {library}
      </Badge>
    {/each}
  {/if}
</BadgeGroup>
