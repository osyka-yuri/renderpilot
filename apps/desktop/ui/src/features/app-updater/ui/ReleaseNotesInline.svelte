<script lang="ts">
  import type { ReleaseNotesInline as Segment } from '@shared/model';

  type Props = {
    segments: Segment[];
  };

  const { segments }: Props = $props();
</script>

{#each segments as segment, segmentIndex (segmentIndex)}
  {#if segment.type === 'strong'}
    <strong class="font-semibold text-foreground">{segment.value}</strong>
  {:else if segment.type === 'code'}
    <code class="rounded-sm bg-muted px-1 py-0.5 font-mono text-[0.85em] text-foreground"
      >{segment.value}</code
    >
  {:else}
    {segment.value}
  {/if}
{/each}
