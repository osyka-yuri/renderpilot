<script lang="ts">
  import { d3d12ExecutableActionIdentity, type D3d12ExecutableMutationAction } from '@shared/model';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { Alert, AlertDescription, Item, ItemContent, ItemSeparator, ItemTitle } from '@shared/ui';
  import { t } from '@shared/i18n';

  let { actions }: { actions: readonly D3d12ExecutableMutationAction[] } = $props();
  const includesIntegrityChange = $derived(
    actions.some(
      (action) =>
        action.kind === 'patch' && action.current_sdk_version === action.original_sdk_version,
    ),
  );
</script>

<div class="grid gap-3">
  <ul class="grid gap-2">
    {#each actions as action (d3d12ExecutableActionIdentity(action))}
      <li>
        <Item variant="outline" size="sm">
          <ItemContent class="min-w-0">
            <ItemTitle>
              {action.kind === 'restore'
                ? t('gameDetails.d3d12.action.planRestore', {
                    from: action.current_sdk_version,
                    to: action.target_sdk_version,
                  })
                : t('gameDetails.d3d12.action.planPatch', {
                    from: action.current_sdk_version,
                    to: action.target_sdk_version,
                  })}
            </ItemTitle>
            <code class="font-mono text-xs wrap-anywhere text-muted-foreground"
              >{action.executable_path}</code
            >
            <ItemSeparator />
            <p class="text-xs wrap-anywhere text-muted-foreground">
              {action.backup_exists
                ? t('gameDetails.d3d12.confirm.backupExists', { path: action.backup_path })
                : t('gameDetails.d3d12.confirm.backupWillCreate', { path: action.backup_path })}
            </p>
          </ItemContent>
        </Item>
      </li>
    {/each}
  </ul>

  {#if includesIntegrityChange}
    <Alert variant="warning" size="sm" role="note">
      <TriangleAlertIcon aria-hidden="true" />
      <AlertDescription>{t('gameDetails.d3d12.confirm.signatureWarning')}</AlertDescription>
    </Alert>
  {/if}
</div>
