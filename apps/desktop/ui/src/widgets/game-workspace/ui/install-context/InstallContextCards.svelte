<script lang="ts">
  import { cn } from '@shared/classnames';
  import { compactList } from '@shared/format';
  import { fileNameFromPath } from '@shared/path';
  import { Badge, Card, CardContent, CardHeader } from '@shared/ui';

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
    <Card>
      <CardHeader>
        <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Folder</p>
      </CardHeader>
      <CardContent>
        <strong
          class="block min-w-0 text-sm/5 font-semibold break-all text-foreground"
          title={installPath}>{installPath}</strong
        >
      </CardContent>
    </Card>

    <Card>
      <CardHeader>
        <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Launch</p>
      </CardHeader>
      <CardContent>
        <strong
          class="block min-w-0 truncate text-sm/5 font-semibold text-foreground"
          title={compactList(launchCandidateNames, 'No executable recorded', 8)}
        >
          {compactList(launchCandidateNames, 'No executable recorded', 2)}
        </strong>
      </CardContent>
    </Card>
  </div>

  <Card>
    <CardHeader>
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Graphics</p>
    </CardHeader>
    <CardContent>
      <div
        class="flex min-w-0 flex-wrap gap-1"
        title={compactList(libraries, 'No graphics libraries detected', 12)}
      >
        {#if libraries.length === 0}
          <Badge variant="outline">None detected</Badge>
        {:else}
          {#each libraries as library (library)}
            <Badge variant="outline">{library}</Badge>
          {/each}
        {/if}
      </div>
    </CardContent>
  </Card>
</section>
