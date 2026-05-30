<script lang="ts">
  import BoxIcon from '@lucide/svelte/icons/box';
  import LibraryIcon from '@lucide/svelte/icons/library';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import SettingsIcon from '@lucide/svelte/icons/settings';
  import type { Component, Snippet } from 'svelte';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';

  import {
    Breadcrumb,
    BreadcrumbItem,
    BreadcrumbLink,
    BreadcrumbList,
    BreadcrumbPage,
    BreadcrumbSeparator,
    Button,
    Sidebar,
    SidebarContent,
    SidebarGroup,
    SidebarInset,
    SidebarMenu,
    SidebarMenuButton,
    SidebarMenuItem,
    SidebarProvider,
    SidebarRail,
    SidebarTrigger,
  } from '@shared/ui';

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

  type PrimaryScreen = Extract<Screen, 'games' | 'libraries' | 'settings'>;

  type NavigationItem = {
    screen: PrimaryScreen;
    label: string;
    tooltip: string;
    icon: Component;
  };

  type BreadcrumbEntry =
    | {
        id: string;
        kind: 'link';
        label: string;
        target: Screen;
      }
    | {
        id: string;
        kind: 'page';
        label: string;
      };

  const NAVIGATION_ITEMS = [
    {
      screen: 'games',
      label: 'Games',
      tooltip: 'Games',
      icon: LibraryIcon,
    },
    {
      screen: 'libraries',
      label: 'Libraries',
      tooltip: 'Libraries',
      icon: BoxIcon,
    },
    {
      screen: 'settings',
      label: 'Settings',
      tooltip: 'Settings',
      icon: SettingsIcon,
    },
  ] satisfies readonly NavigationItem[];

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

  const resolvedGameTitle = $derived(selectedGameTitle?.trim() ?? 'Game');

  const breadcrumbs = $derived(createBreadcrumbs(screen, resolvedGameTitle));

  function createBreadcrumbs(currentScreen: Screen, gameTitle: string): BreadcrumbEntry[] {
    switch (currentScreen) {
      case 'games':
        return [{ id: 'games-page', kind: 'page', label: 'Games' }];

      case 'settings':
        return [{ id: 'settings-page', kind: 'page', label: 'Settings' }];

      case 'libraries':
        return [{ id: 'libraries-page', kind: 'page', label: 'Libraries' }];

      case 'details':
        return [
          { id: 'games-link', kind: 'link', label: 'Games', target: 'games' },
          { id: 'game-page', kind: 'page', label: gameTitle },
        ];

      case 'operations':
        return [
          { id: 'games-link', kind: 'link', label: 'Games', target: 'games' },
          { id: 'game-link', kind: 'link', label: gameTitle, target: 'details' },
          { id: 'operations-page', kind: 'page', label: 'Operations' },
        ];

      default: {
        return [{ id: 'fallback-games-page', kind: 'page', label: 'Games' }];
      }
    }
  }
</script>

<SidebarProvider bind:open={sidebarOpen}>
  <Sidebar collapsible="icon" variant="sidebar">
    <SidebarContent>
      <SidebarGroup>
        <SidebarMenu>
          {#each NAVIGATION_ITEMS as item (item.screen)}
            {@const Icon = item.icon}

            <SidebarMenuItem>
              <SidebarMenuButton
                isActive={screen === item.screen}
                onclick={() => {
                  onNavigate(item.screen);
                }}
                tooltipContent={item.tooltip}
              >
                <Icon />
                <span>{item.label}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          {/each}
        </SidebarMenu>
      </SidebarGroup>
    </SidebarContent>

    <SidebarRail />
  </Sidebar>

  <SidebarInset class="min-h-0 overflow-hidden">
    <header class="flex shrink-0 items-center gap-2 border-b px-4 py-2">
      <SidebarTrigger />

      <Breadcrumb>
        <BreadcrumbList>
          {#each breadcrumbs as item, index (item.id)}
            {#if index > 0}
              <BreadcrumbSeparator />
            {/if}

            <BreadcrumbItem>
              {#if item.kind === 'link'}
                <BreadcrumbLink
                  href={`#${item.target}`}
                  onclick={(event: MouseEvent) => {
                    event.preventDefault();
                    onNavigate(item.target);
                  }}
                >
                  {item.label}
                </BreadcrumbLink>
              {:else}
                <BreadcrumbPage>{item.label}</BreadcrumbPage>
              {/if}
            </BreadcrumbItem>
          {/each}
        </BreadcrumbList>
      </Breadcrumb>

      <div class="ml-auto">
        <Button
          variant="outline"
          size="icon"
          disabled={busy}
          onclick={onRefresh}
          aria-label="Refresh"
        >
          <RefreshCwIcon class={refreshing ? 'animate-spin' : ''} aria-hidden="true" />
        </Button>
      </div>
    </header>

    {@render banner?.()}

    <main class="grid min-h-0 flex-1 grid-rows-[1fr] gap-4 overflow-hidden p-4" aria-busy={busy}>
      {@render children?.()}
    </main>
  </SidebarInset>
</SidebarProvider>
