<script lang="ts">
  import { onMount } from 'svelte';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import {
    Alert,
    AlertDescription,
    Badge,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemTitle,
    Switch,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { DlssIndicatorContext } from '@features/nvapi-settings';

  type Props = {
    dlssIndicator: DlssIndicatorContext;
  };

  const { dlssIndicator }: Props = $props();

  onMount(() => {
    if (!dlssIndicator.loaded) {
      void dlssIndicator.load();
    }
  });
</script>

{#if dlssIndicator.supported}
  <Card>
    <CardHeader class="pb-2">
      <div class="flex items-start justify-between gap-3">
        <div class="grid min-w-0 gap-1">
          <CardTitle>{t('settings.nvidia.indicator.title')}</CardTitle>
          <CardDescription>
            {t('settings.nvidia.indicator.description')}
          </CardDescription>
        </div>
        <Badge variant="secondary" class="shrink-0"
          >{t('settings.nvidia.indicator.systemWide')}</Badge
        >
      </div>
    </CardHeader>

    <CardContent class="grid gap-2">
      {#if !dlssIndicator.canWrite}
        <Alert variant="warning" size="sm" role="note">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertDescription>
            {t('settings.nvidia.indicator.adminRequired')}
          </AlertDescription>
        </Alert>
      {/if}

      {#if dlssIndicator.error}
        <div
          class="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm text-destructive"
        >
          {dlssIndicator.error}
        </div>
      {/if}

      <ItemGroup>
        <Item size="sm" variant="outline" class="rounded-md bg-muted/30">
          <ItemContent>
            <ItemTitle>{t('settings.nvidia.indicator.overlayTitle')}</ItemTitle>
            <ItemDescription>{t('settings.nvidia.indicator.overlayDescription')}</ItemDescription>
          </ItemContent>
          <ItemActions>
            <Switch
              checked={dlssIndicator.enabled}
              disabled={!dlssIndicator.canWrite || dlssIndicator.busy}
              aria-label={t('settings.nvidia.indicator.toggleAria')}
              onCheckedChange={(checked: boolean) => {
                void dlssIndicator.setEnabled(checked);
              }}
            />
          </ItemActions>
        </Item>
      </ItemGroup>
    </CardContent>
  </Card>
{/if}
