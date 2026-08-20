<script lang="ts">
  import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
  import { tick, type Snippet } from 'svelte';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';
  import { t } from '@shared/i18n';
  import { Button, SidebarProvider, SidebarInset } from '@shared/ui';
  import { createShellNavigation } from '@app/navigation/shell-navigation';

  import ShellSidebar from './ShellSidebar.svelte';
  import ShellHeader from './ShellHeader.svelte';

  type Props = {
    screen: Screen;
    busy?: boolean;
    refreshing?: boolean;
    selectedGameTitle?: string | null;
    onNavigate?: ScreenHandler;
    onPreload?: ScreenHandler;
    onRefresh?: () => void;
    updateAvailable?: boolean;
    updateOpening?: boolean;
    onOpenUpdate?: () => void;
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
    onPreload = () => undefined,
    onRefresh = () => undefined,
    updateAvailable = false,
    updateOpening = false,
    onOpenUpdate = () => undefined,
    banner,
    children,
  }: Props = $props();

  let sidebarOpen = $state(false);
  let previousScreen: Screen | undefined;

  const resolvedGameTitle = $derived(selectedGameTitle?.trim() ?? t('nav.gameFallback'));
  const shellNavigation = $derived(createShellNavigation(screen, resolvedGameTitle));

  $effect(() => {
    const currentScreen = screen;
    const shouldMoveFocus = previousScreen !== undefined && previousScreen !== currentScreen;
    previousScreen = currentScreen;

    if (!shouldMoveFocus) {
      return;
    }

    let cancelled = false;

    void tick().then(() => {
      if (!cancelled) {
        document.getElementById('main-content')?.focus({ preventScroll: true });
      }
    });

    return () => {
      cancelled = true;
    };
  });
</script>

<svelte:head>
  <title>{t('shell.pageTitle', { page: shellNavigation.pageLabel })}</title>
</svelte:head>

<SidebarProvider bind:open={sidebarOpen}>
  <Button
    href="#main-content"
    variant="outline"
    size="sm"
    class="fixed top-0 left-1/2 z-100 -translate-x-1/2 -translate-y-full shadow-sm transition-transform duration-150 focus:translate-y-2 focus-visible:ring-2 motion-reduce:transition-none"
  >
    <ArrowDownIcon class="size-3.5" aria-hidden="true" />
    {t('nav.skipToContent')}
  </Button>

  <ShellSidebar navigation={shellNavigation} {onNavigate} {onPreload} />

  <SidebarInset class="min-h-0 overflow-hidden">
    <ShellHeader
      navigation={shellNavigation}
      {busy}
      {refreshing}
      {onNavigate}
      {onPreload}
      {onRefresh}
      {updateAvailable}
      {updateOpening}
      {onOpenUpdate}
    />

    {@render banner?.()}

    <main
      id="main-content"
      tabindex="-1"
      class="grid min-h-0 flex-1 grid-rows-[1fr] gap-4 overflow-hidden p-4 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
      aria-label={shellNavigation.pageLabel}
      aria-busy={busy}
    >
      {@render children?.()}
    </main>
  </SidebarInset>
</SidebarProvider>
