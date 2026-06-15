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
  import { t, type MessageKey } from '@shared/i18n';
  import {
    NvapiSettingGroup,
    type DlssIndicatorContext,
    type GlobalNvidiaPresetsContext,
    type SettingFamily,
    type SettingStateResponse,
  } from '@features/nvapi-settings';

  type Props = {
    dlssIndicator: DlssIndicatorContext;
    globalPresets: GlobalNvidiaPresetsContext;
  };

  const { dlssIndicator, globalPresets }: Props = $props();

  // DLSS families rendered in the global settings card, in catalog order.
  const families: { family: SettingFamily; titleKey: MessageKey }[] = [
    { family: 'sr', titleKey: 'settings.nvidia.global.familySr' },
    { family: 'fg', titleKey: 'settings.nvidia.global.familyFg' },
    { family: 'rr', titleKey: 'settings.nvidia.global.familyRr' },
  ];

  function rowDisabled(state: SettingStateResponse): boolean {
    return (
      !globalPresets.canWrite || globalPresets.busy || globalPresets.isPending(state.setting_key)
    );
  }

  onMount(() => {
    if (!dlssIndicator.loaded) {
      void dlssIndicator.load();
    }
    if (!globalPresets.loaded) {
      void globalPresets.load();
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

{#if globalPresets.nvapiAvailable}
  <Card>
    <CardHeader class="pb-2">
      <div class="flex items-start justify-between gap-3">
        <div class="grid min-w-0 gap-1">
          <CardTitle>{t('settings.nvidia.global.title')}</CardTitle>
          <CardDescription>
            {t('settings.nvidia.global.description')}
          </CardDescription>
        </div>
        <Badge variant="secondary" class="shrink-0">{t('settings.nvidia.global.systemWide')}</Badge>
      </div>
    </CardHeader>

    <CardContent class="grid gap-4">
      {#if globalPresets.loadError}
        <div
          class="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm text-destructive"
        >
          {globalPresets.loadError}
        </div>
      {/if}

      {#each globalPresets.profileWarnings as warning (warning)}
        <Alert variant="warning" size="sm" role="note">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertDescription>{warning}</AlertDescription>
        </Alert>
      {/each}

      {#each families as { family, titleKey } (family)}
        {@const settings = globalPresets.settingsForFamily(family)}
        {#if settings.length > 0}
          <div class="grid gap-1.5">
            <div class="px-1 text-xs font-medium text-muted-foreground">{t(titleKey)}</div>
            <NvapiSettingGroup
              {settings}
              warnings={globalPresets.familyWarnings(family)}
              canWrite={globalPresets.canWrite}
              adminMessage={t('settings.nvidia.global.adminRequired')}
              {rowDisabled}
              onChange={(key: string, wire: string) => {
                void globalPresets.setValue(key, wire);
              }}
              onRevertPredefined={(key: string) => {
                void globalPresets.revertDefault(key);
              }}
            />
          </div>
        {/if}
      {/each}
    </CardContent>
  </Card>
{/if}
