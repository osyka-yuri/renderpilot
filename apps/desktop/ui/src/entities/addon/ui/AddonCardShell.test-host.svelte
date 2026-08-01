<script lang="ts">
  import type { MessageKeyWithoutParams } from '@shared/i18n';

  import AddonCardShell from './AddonCardShell.svelte';

  type Props = {
    title?: string;
    description?: string;
    loadingLabel?: string;
    progressActive?: boolean;
    actionsDisabled?: boolean;
    showLoading?: boolean;
    showLoadError?: boolean;
    retrying?: boolean;
    showAttribution?: boolean;
    attributionHref?: string;
    onRetry: () => void;
    body?: string;
    headerLabel?: string | null;
  };

  type Attribution = {
    textKey: MessageKeyWithoutParams;
    linkKey: MessageKeyWithoutParams;
    href: string;
  };

  const {
    title = 'Card title',
    description = 'Card description',
    loadingLabel = 'Loading card…',
    progressActive = false,
    actionsDisabled = false,
    showLoading = false,
    showLoadError = false,
    retrying = false,
    showAttribution = false,
    attributionHref = 'https://example.test/project',
    onRetry,
    body = 'shell-body',
    headerLabel = null,
  }: Props = $props();

  const attribution = $derived<Attribution>({
    textKey: 'gameDetails.luma.attribution',
    linkKey: 'gameDetails.luma.attributionLink',
    href: attributionHref,
  });
</script>

<AddonCardShell
  {title}
  {description}
  {loadingLabel}
  progressIds={['game-1']}
  {progressActive}
  {actionsDisabled}
  {showLoading}
  {showLoadError}
  {retrying}
  {showAttribution}
  {attribution}
  {onRetry}
>
  {#snippet headerActions()}
    {#if headerLabel}
      <span data-testid="header-action">{headerLabel}</span>
    {/if}
  {/snippet}

  <p data-testid="shell-body">{body}</p>
</AddonCardShell>
