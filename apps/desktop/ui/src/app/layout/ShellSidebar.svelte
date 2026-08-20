<script lang="ts">
  import BoxIcon from '@lucide/svelte/icons/box';
  import LibraryIcon from '@lucide/svelte/icons/library';
  import SettingsIcon from '@lucide/svelte/icons/settings';
  import type { Component } from 'svelte';

  import type { ScreenHandler } from '@app/navigation/screen';
  import type {
    ShellNavigation,
    ShellPrimaryNavigationItem,
  } from '@app/navigation/shell-navigation';
  import { t } from '@shared/i18n';
  import {
    Sidebar,
    SidebarContent,
    SidebarGroup,
    SidebarMenu,
    SidebarMenuButton,
    SidebarMenuItem,
    SidebarRail,
    useSidebar,
  } from '@shared/ui';

  type Props = {
    navigation: ShellNavigation;
    onNavigate: ScreenHandler;
    onPreload: ScreenHandler;
  };

  const { navigation, onNavigate, onPreload }: Props = $props();
  const sidebar = useSidebar();

  const ICONS = {
    games: LibraryIcon,
    libraries: BoxIcon,
    settings: SettingsIcon,
  } satisfies Record<ShellPrimaryNavigationItem['screen'], Component>;

  function handleNavigation(event: MouseEvent, target: ShellPrimaryNavigationItem['screen']): void {
    event.preventDefault();
    sidebar.setOpenMobile(false);
    onNavigate(target);
  }
</script>

<Sidebar
  collapsible="icon"
  variant="sidebar"
  mobileTitle={t('shell.sidebar.title')}
  mobileDescription={t('shell.sidebar.description')}
  mobileCloseLabel={t('common.close')}
>
  <SidebarContent>
    <SidebarGroup>
      <nav aria-label={navigation.primaryNavigationLabel}>
        <SidebarMenu>
          {#each navigation.primaryNavigation as item (item.screen)}
            {@const Icon = ICONS[item.screen]}

            <SidebarMenuItem>
              <SidebarMenuButton isActive={item.isActive} tooltipContent={item.label}>
                {#snippet child({ props })}
                  <a
                    {...props}
                    href={`#${item.screen}`}
                    aria-current={item.ariaCurrent}
                    onclick={(event: MouseEvent) => {
                      handleNavigation(event, item.screen);
                    }}
                    onpointerenter={() => {
                      onPreload(item.screen);
                    }}
                    onfocus={() => {
                      onPreload(item.screen);
                    }}
                  >
                    <Icon aria-hidden="true" />
                    <span>{item.label}</span>
                  </a>
                {/snippet}
              </SidebarMenuButton>
            </SidebarMenuItem>
          {/each}
        </SidebarMenu>
      </nav>
    </SidebarGroup>
  </SidebarContent>

  <SidebarRail label={t('shell.sidebar.toggle')} />
</Sidebar>
