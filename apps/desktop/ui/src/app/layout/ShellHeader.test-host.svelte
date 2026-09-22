<script lang="ts">
  import { createShellNavigation } from '@app/navigation/shell-navigation';
  import { SidebarProvider } from '@shared/ui';
  import ShellHeader from './ShellHeader.svelte';

  type Props = {
    busy?: boolean;
    refreshing?: boolean;
    onRefresh?: () => void;
  };

  const { busy = false, refreshing = false, onRefresh = () => undefined }: Props = $props();

  const navigation = createShellNavigation('games', 'RenderPilot');
</script>

<SidebarProvider>
  <ShellHeader
    {navigation}
    {busy}
    {refreshing}
    onNavigate={() => undefined}
    onPreload={() => undefined}
    {onRefresh}
  />
</SidebarProvider>
