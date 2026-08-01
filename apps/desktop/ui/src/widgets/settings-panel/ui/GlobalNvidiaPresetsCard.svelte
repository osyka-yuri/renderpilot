<script lang="ts">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import CpuIcon from '@lucide/svelte/icons/cpu';
  import {
    Alert,
    AlertDescription,
    Badge,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
  } from '@shared/ui';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import {
    NvapiSettingGroup,
    type GlobalNvidiaPresetsContext,
    type SettingFamily,
    type SettingStateResponse,
  } from '@features/nvapi-settings';

  type Props = {
    globalPresets: GlobalNvidiaPresetsContext;
  };

  const { globalPresets }: Props = $props();

  // DLSS families rendered in the global settings card, in catalog order.
  const families: { family: SettingFamily; titleKey: MessageKeyWithoutParams }[] = [
    { family: 'sr', titleKey: 'settings.nvidia.global.familySr' },
    { family: 'fg', titleKey: 'settings.nvidia.global.familyFg' },
    { family: 'rr', titleKey: 'settings.nvidia.global.familyRr' },
  ];

  function rowDisabled(state: SettingStateResponse): boolean {
    return (
      !globalPresets.canWrite || globalPresets.busy || globalPresets.isPending(state.setting_key)
    );
  }
</script>

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
      <Alert variant="destructive" size="sm" role="alert">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{globalPresets.loadError}</AlertDescription>
      </Alert>
    {/if}

    <!-- One card-level admin notice instead of one per family group. -->
    {#if globalPresets.hasStates && !globalPresets.canWrite}
      <Alert variant="warning" size="sm" role="note">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{t('settings.nvidia.global.adminRequired')}</AlertDescription>
      </Alert>
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
        {@const dllInfo = globalPresets.dllInfoForFamily(family)}
        <div class="grid gap-1.5">
          <div class="flex items-center justify-between gap-2 px-1">
            <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
              <CpuIcon class="size-3.5" aria-hidden="true" />
              <span>{t(titleKey)}</span>
            </div>
            {#if dllInfo}
              <span class="truncate text-xs text-muted-foreground">
                {dllInfo.manifest_label ?? `DLSS ${dllInfo.version}`} · v{dllInfo.version}
              </span>
            {/if}
          </div>
          <NvapiSettingGroup
            {settings}
            warnings={globalPresets.familyWarnings(family)}
            canWrite={globalPresets.canWrite}
            adminMessage={t('settings.nvidia.global.adminRequired')}
            showAdminWarning={false}
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
