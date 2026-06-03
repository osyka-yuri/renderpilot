<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';
  import { t } from '@shared/i18n';
  import { SidebarProvider, SidebarInset } from '@shared/ui';
  
  import ShellSidebar from './ShellSidebar.svelte';
  import ShellHeader from './ShellHeader.svelte';

  type Props = {
    screen: Screen;
    busy?: boolean;
    refreshing?: boolean;
    selectedGameTitle?: string | null;
    onNavigate?: ScreenHandler;
    onRefresh?: () => void;
    /** Optional banner rendered between the header and main content area,
     *  inside SidebarInset so it is never obscured by the sidebar overlay. */
    banner?: Snippet;
    children?: Snippet;
  };

  const {
    screen,
    busy = false,
    refreshing = false,
    selectedGameTitle = null,
    onNavigate = () => undefined,
    onRefresh = () => undefined,
    banner,
    children,
  }: Props = $props();

  let sidebarOpen = $state(false);

  const resolvedGameTitle = $derived(selectedGameTitle?.trim() ?? t('nav.gameFallback'));
</script>

<SidebarProvider bind:open={sidebarOpen}>
  <ShellSidebar {screen} {onNavigate} />

  <SidebarInset class="min-h-0 overflow-hidden">
    <ShellHeader 
      {screen} 
      {resolvedGameTitle} 
      {busy} 
      {refreshing} 
      {onNavigate} 
      {onRefresh} 
    />

    {@render banner?.()}

    <main class="grid min-h-0 flex-1 grid-rows-[1fr] gap-4 overflow-hidden p-4" aria-busy={busy}>
      {@render children?.()}
    </main>
  </SidebarInset>
</SidebarProvider>
