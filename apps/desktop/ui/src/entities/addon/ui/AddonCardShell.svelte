<script lang="ts">
  import type { Snippet } from 'svelte';

  import type { MessageKeyWithoutParams } from '@shared/i18n';
  import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
    DownloadProgressBar,
    Spinner,
  } from '@shared/ui';

  import AddonAvailabilityFailure from './AddonAvailabilityFailure.svelte';
  import AddonAttribution from './AddonAttribution.svelte';

  type Props = {
    title: string;
    description: string;
    loadingLabel: string;
    progressIds: string[];
    /**
     * This card's own operation busy flag. Must not include page-level peer
     * busy: both cards share `gameId` progress ids and would otherwise show
     * each other's download phases.
     */
    progressActive: boolean;
    /** Combined page+store busy for disabling the load-error retry control. */
    actionsDisabled: boolean;
    /** Initial card load spinner (view === 'loading'). */
    showLoading: boolean;
    /** Availability failed to load (view === 'load-error'). */
    showLoadError: boolean;
    /** In-flight load used for the failure retry button spinner. */
    retrying: boolean;
    showAttribution: boolean;
    attribution: {
      textKey: MessageKeyWithoutParams;
      linkKey: MessageKeyWithoutParams;
      href: string;
    };
    onRetry: () => void;
    headerActions?: Snippet;
    children: Snippet;
  };

  const {
    title,
    description,
    loadingLabel,
    progressIds,
    progressActive,
    actionsDisabled,
    showLoading,
    showLoadError,
    retrying,
    showAttribution,
    attribution,
    onRetry,
    headerActions,
    children,
  }: Props = $props();
</script>

<Card>
  <CardHeader class="pb-2">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0 space-y-1.5">
        <CardTitle level={2}>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </div>

      <div class="flex shrink-0 items-start gap-1">
        <DownloadProgressBar ids={progressIds} active={progressActive} class="shrink-0" />
        {#if headerActions}
          {@render headerActions()}
        {/if}
      </div>
    </div>
  </CardHeader>

  <CardContent class="flex w-full flex-col gap-4">
    {#if showLoading}
      <div class="flex items-center gap-2 text-sm text-muted-foreground">
        <Spinner class="size-4" />
        <span>{loadingLabel}</span>
      </div>
    {:else if showLoadError}
      <AddonAvailabilityFailure {retrying} disabled={actionsDisabled} {onRetry} />
    {:else}
      {@render children()}
    {/if}
  </CardContent>

  {#if showAttribution}
    <CardFooter class="mt-auto">
      <AddonAttribution {...attribution} />
    </CardFooter>
  {/if}
</Card>
