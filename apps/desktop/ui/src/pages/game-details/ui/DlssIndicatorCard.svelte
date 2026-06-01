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
          <CardTitle>DLSS indicator</CardTitle>
          <CardDescription>
            A diagnostic overlay that draws the active DLSS version, preset, and mode in the corner
            of the screen while a game runs.
          </CardDescription>
        </div>
        <Badge variant="secondary" class="shrink-0">System-wide</Badge>
      </div>
    </CardHeader>

    <CardContent class="grid gap-2">
      {#if !indicator.canWrite}
        <Alert variant="warning" size="sm" role="note">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertDescription>
            Changing this requires administrator privileges — use the banner at the top of the
            window to relaunch.
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

      <Item size="sm" variant="muted" class="rounded-md">
        <ItemContent>
          <ItemTitle>On-screen DLSS overlay</ItemTitle>
          <ItemDescription>Affects every DLSS game on this PC, not just this one.</ItemDescription>
        </ItemContent>
        <ItemActions>
          <Switch
            checked={indicator.enabled}
            disabled={!indicator.canWrite || indicator.busy}
            aria-label="Toggle the system-wide DLSS indicator overlay"
            onCheckedChange={(checked: boolean) => {
              void indicator.setEnabled(checked);
            }}
          />
        </ItemActions>
      </Item>
    </CardContent>
  </Card>
{/if}
