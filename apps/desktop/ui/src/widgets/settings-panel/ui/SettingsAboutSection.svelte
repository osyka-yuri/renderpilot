<script lang="ts">
  import {
    Button,
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
    Spinner,
  } from '@shared/ui';
  import { t } from '@shared/i18n';

  type Props = {
    appVersion?: string | null;
    isCheckingForUpdates?: boolean;
    isDownloading?: boolean;
    onCheckForUpdates?: () => void;
  };

  const {
    appVersion = null,
    isCheckingForUpdates = false,
    isDownloading = false,
    onCheckForUpdates = () => undefined,
  }: Props = $props();
</script>

<Card>
  <CardHeader>
    <CardTitle>{t('settings.about.title')}</CardTitle>
    <CardDescription>{t('settings.about.description')}</CardDescription>
  </CardHeader>
  <CardContent>
    <ItemGroup>
      <Item>
        <ItemContent>
          <ItemTitle>{t('settings.about.version.title')}</ItemTitle>
          <ItemDescription>
            {#if appVersion}
              RenderPilot v{appVersion}
            {:else}
              {t('settings.about.version.loading')}
            {/if}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          <Button
            variant="secondary"
            size="sm"
            disabled={isCheckingForUpdates || isDownloading}
            onclick={onCheckForUpdates}
          >
            {#if isCheckingForUpdates || isDownloading}
              <Spinner class="mr-2" />
            {/if}
            {#if isDownloading}
              {t('settings.about.downloading')}
            {:else}
              {t('settings.about.checkUpdates')}
            {/if}
          </Button>
        </ItemActions>
      </Item>
    </ItemGroup>
  </CardContent>
</Card>
