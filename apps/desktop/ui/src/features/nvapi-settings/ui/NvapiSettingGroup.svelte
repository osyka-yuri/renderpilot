<script lang="ts">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { Alert, AlertDescription, ItemGroup, ItemSeparator } from '@shared/ui';
  import type { SettingStateResponse } from '../model/types';
  import NvapiSettingRow from './NvapiSettingRow.svelte';

  type Props = {
    /** Settings to render as rows (already filtered to one family by the caller). */
    settings: SettingStateResponse[];
    /** Family-level warnings to surface above the rows. */
    warnings: string[];
    /** Whether NVAPI writes can succeed (admin); gates the admin-required alert. */
    canWrite: boolean;
    /** Message shown when `canWrite` is false. */
    adminMessage: string;
    /**
     * Whether this group renders its own admin-required alert when `!canWrite`.
     * Defaults to `true`. Set to `false` when several groups share one card and
     * the caller surfaces a single card-level alert instead (avoids duplicates).
     */
    showAdminWarning?: boolean;
    /** Whether a given setting's row should be disabled. */
    rowDisabled: (state: SettingStateResponse) => boolean;
    onChange: (key: string, wire: string) => void;
    onRevertPredefined: (key: string) => void;
    /**
     * Restore the pre-RenderPilot baseline. Optional: the global/base profile
     * has no baseline, so its row's restore button is always disabled and this
     * never fires.
     */
    onRevertBaseline?: (key: string) => void;
  };

  const {
    settings,
    warnings,
    canWrite,
    adminMessage,
    showAdminWarning = true,
    rowDisabled,
    onChange,
    onRevertPredefined,
    onRevertBaseline = () => undefined,
  }: Props = $props();
</script>

{#if showAdminWarning && !canWrite}
  <Alert variant="warning" size="sm" role="note">
    <TriangleAlertIcon aria-hidden="true" />
    <AlertDescription>{adminMessage}</AlertDescription>
  </Alert>
{/if}

{#each warnings as warning (warning)}
  <Alert variant="warning" size="sm" role="note">
    <TriangleAlertIcon aria-hidden="true" />
    <AlertDescription>{warning}</AlertDescription>
  </Alert>
{/each}

<ItemGroup class="rounded-md border bg-muted/30">
  {#each settings as state, index (state.setting_key)}
    {#if index > 0}
      <ItemSeparator />
    {/if}
    <NvapiSettingRow
      {state}
      disabled={rowDisabled(state)}
      onChange={(wire: string) => {
        onChange(state.setting_key, wire);
      }}
      onRevertPredefined={() => {
        onRevertPredefined(state.setting_key);
      }}
      onRevertBaseline={() => {
        onRevertBaseline(state.setting_key);
      }}
    />
  {/each}
</ItemGroup>
