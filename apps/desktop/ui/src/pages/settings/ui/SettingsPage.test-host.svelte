<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import { TooltipProvider } from '@shared/ui';
  import SettingsPage from './SettingsPage.svelte';

  const props: ComponentProps<typeof SettingsPage> = $props();
</script>

<TooltipProvider delayDuration={0}>
  <SettingsPage {...props} />
</TooltipProvider>
