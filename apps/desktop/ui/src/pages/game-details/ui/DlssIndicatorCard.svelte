<script lang="ts">
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
    ItemTitle,
    Switch,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { DlssIndicatorContext } from '../model/create-dlss-indicator-context.svelte';

  type Props = {
    indicator: DlssIndicatorContext;
  };

  const { indicator }: Props = $props();
</script>

{#if indicator.supported}
  <Card>
    <CardHeader class="pb-2">
      <div class="flex items-start justify-between gap-3">
        <div class="grid min-w-0 gap-1">
          <CardTitle>{t('gameDetails.indicator.title')}</CardTitle>
          <CardDescription>
            {t('gameDetails.indicator.description')}
          </CardDescription>
        </div>
        <Badge variant="secondary" class="shrink-0">{t('gameDetails.indicator.systemWide')}</Badge>
      </div>
    </CardHeader>

    <CardContent class="grid gap-2">
      {#if !indicator.canWrite}
        <Alert variant="warning" size="sm" role="note">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertDescription>
            {t('gameDetails.indicator.adminRequired')}
          </AlertDescription>
        </Alert>
      {/if}

      {#if indicator.error}
        <div
          class="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm text-destructive"
        >
          {indicator.error}
        </div>
      {/if}

      <Item size="sm" variant="outline" class="rounded-md bg-muted/30">
        <ItemContent>
          <ItemTitle>{t('gameDetails.indicator.overlayTitle')}</ItemTitle>
          <ItemDescription>{t('gameDetails.indicator.overlayDescription')}</ItemDescription>
        </ItemContent>
        <ItemActions>
          <Switch
            checked={indicator.enabled}
            disabled={!indicator.canWrite || indicator.busy}
            aria-label={t('gameDetails.indicator.toggleAria')}
            onCheckedChange={(checked: boolean) => {
              void indicator.setEnabled(checked);
            }}
          />
        </ItemActions>
      </Item>
    </CardContent>
  </Card>
{/if}
