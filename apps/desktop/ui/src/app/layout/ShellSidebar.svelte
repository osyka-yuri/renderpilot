<script lang="ts">
  import BoxIcon from '@lucide/svelte/icons/box';
  import LibraryIcon from '@lucide/svelte/icons/library';
  import SettingsIcon from '@lucide/svelte/icons/settings';
  import type { Component } from 'svelte';

  import type { ScreenHandler, Screen } from '@app/navigation/screen';
  import { t, type MessageKey } from '@shared/i18n';
  import {
    Sidebar,
    SidebarContent,
    SidebarGroup,
    SidebarMenu,
    SidebarMenuButton,
    SidebarMenuItem,
    SidebarRail,
  } from '@shared/ui';

  type Props = {
    screen: Screen;
    onNavigate: ScreenHandler;
  };

  const { screen, onNavigate }: Props = $props();

  type PrimaryScreen = Extract<Screen, 'games' | 'libraries' | 'settings'>;

  type NavigationItem = {
    screen: PrimaryScreen;
    labelKey: MessageKey;
    icon: Component;
  };

  const NAVIGATION_ITEMS = [
    {
      screen: 'games',
      labelKey: 'nav.games',
      icon: LibraryIcon,
    },
    {
      screen: 'libraries',
      labelKey: 'nav.libraries',
      icon: BoxIcon,
    },
    {
      screen: 'settings',
      labelKey: 'nav.settings',
      icon: SettingsIcon,
    },
  ] satisfies readonly NavigationItem[];
</script>

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
              tooltipContent={t(item.labelKey)}
            >
              <Icon />
              <span>{t(item.labelKey)}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        {/each}
      </SidebarMenu>
    </SidebarGroup>
  </SidebarContent>

  <SidebarRail />
</Sidebar>
