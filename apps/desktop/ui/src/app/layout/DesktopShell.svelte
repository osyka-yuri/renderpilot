<script lang="ts">
  import LibraryIcon from '@lucide/svelte/icons/library';
  import SettingsIcon from '@lucide/svelte/icons/settings';
  import type { Snippet } from 'svelte';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';

  import {
    Badge,
    Breadcrumb,
    BreadcrumbItem,
    BreadcrumbLink,
    BreadcrumbList,
    BreadcrumbPage,
    BreadcrumbSeparator,
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
    selectedGameTitle?: string | null;
    onNavigate?: ScreenHandler;
    children?: Snippet;
  };

  const {
    screen,
    busy = false,
    selectedGameTitle = null,
    onNavigate = () => undefined,
    children,
  }: Props = $props();

  let sidebarOpen = $state(false);

  const resolvedGameTitle = $derived(selectedGameTitle?.trim() ?? 'Game');
</script>

<SidebarProvider bind:open={sidebarOpen}>
  <Sidebar collapsible="icon" variant="sidebar">
    <SidebarContent>
      <SidebarGroup>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              isActive={screen === 'games'}
              onclick={() => {
                onNavigate('games');
              }}
              tooltipContent="Games"
            >
              <LibraryIcon />
              <span>Games</span>
            </SidebarMenuButton>
          </SidebarMenuItem>

          <SidebarMenuItem>
            <SidebarMenuButton
              isActive={screen === 'settings'}
              onclick={() => {
                onNavigate('settings');
              }}
              tooltipContent="Settings"
            >
              <SettingsIcon />
              <span>Settings</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarGroup>
    </SidebarContent>
    <SidebarRail />
  </Sidebar>

  <SidebarInset>
    <header class="flex items-center gap-2 border-b px-4 py-2">
      <SidebarTrigger />

      <Breadcrumb>
        <BreadcrumbList>
          {#if screen === 'games'}
            <BreadcrumbItem>
              <BreadcrumbPage>Games</BreadcrumbPage>
            </BreadcrumbItem>
          {:else if screen === 'settings'}
            <BreadcrumbItem>
              <BreadcrumbPage>Settings</BreadcrumbPage>
            </BreadcrumbItem>
          {:else}
            <BreadcrumbItem>
              <BreadcrumbLink
                href="#"
                onclick={(e: MouseEvent) => {
                  e.preventDefault();
                  onNavigate('games');
                }}
              >
                Games
              </BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator />

            {#if screen === 'details'}
              <BreadcrumbItem>
                <BreadcrumbPage>{resolvedGameTitle}</BreadcrumbPage>
              </BreadcrumbItem>
            {:else if screen === 'operations'}
              <BreadcrumbItem>
                <BreadcrumbLink
                  href="#"
                  onclick={(e: MouseEvent) => {
                    e.preventDefault();
                    onNavigate('details');
                  }}
                >
                  {resolvedGameTitle}
                </BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>Operations</BreadcrumbPage>
              </BreadcrumbItem>
            {/if}
          {/if}
        </BreadcrumbList>
      </Breadcrumb>

      {#if busy}
        <span class="ml-auto inline-flex" role="status" aria-live="polite">
          <Badge variant="secondary">
            <span class="size-1.5 rounded-full bg-current" aria-hidden="true"></span>
            Working
          </Badge>
        </span>
      {/if}
    </header>

    <main class="grid min-w-0 gap-4 p-4">
      {@render children?.()}
    </main>
  </SidebarInset>
</SidebarProvider>
