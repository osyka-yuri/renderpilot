<script lang="ts">
  import { cn, compactList, fileNameFromPath } from '@shared/utils';
  import { Badge, InfoTile } from '@shared/ui';

  type Props = {
    installPath?: string;
    launchCandidates?: string[];
    libraries?: string[];
  };

  const { installPath = '', launchCandidates = [], libraries = [] }: Props = $props();

  const launchCandidateNames = $derived(launchCandidates.map(fileNameFromPath));
</script>

<section
  class={cn('grid grid-cols-2 items-stretch gap-2', 'max-lg:grid-cols-1')}
  aria-label="Game installation context"
>
  <div class="grid min-w-0 gap-2">
    <InfoTile label="Folder" tone="card" class="min-h-18 gap-2">
      <strong
        class="min-w-0 truncate text-sm/tight font-semibold text-text-strong"
        title={installPath}>{installPath}</strong
      >
    </InfoTile>

    <InfoTile label="Launch" tone="card" class="min-h-18 gap-2">
      <strong
        class="min-w-0 truncate text-sm/tight font-semibold text-text-strong"
        title={compactList(launchCandidateNames, 'No executable recorded', 8)}
      >
        {compactList(launchCandidateNames, 'No executable recorded', 2)}
      </strong>
    </InfoTile>
  </div>

  <InfoTile label="Graphics" tone="card" class="min-h-full gap-2">
    <div
      class="flex min-w-0 flex-wrap gap-1"
      title={compactList(libraries, 'No graphics libraries detected', 12)}
    >
      {#if libraries.length === 0}
        <Badge surface="outline" tone="muted">None detected</Badge>
      {:else}
        {#each libraries as library (library)}
          <Badge surface="outline">{library}</Badge>
        {/each}
      {/if}
    </div>
  </InfoTile>
</section>
