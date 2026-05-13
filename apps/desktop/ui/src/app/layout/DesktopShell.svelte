<script lang="ts">
  import ArrowLeftIcon from '@lucide/svelte/icons/arrow-left';
  import Settings2Icon from '@lucide/svelte/icons/settings-2';
  import type { Snippet } from 'svelte';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';

  import { Badge, Button, Card, CardContent } from '@shared/ui';

  type WorkspaceCopy = {
    label: string;
    heading: string;
  };

  type Props = {
    screen: Screen;
    busy?: boolean;
    selectedGameTitle?: string | null;
    onNavigate?: ScreenHandler;
    onBack?: () => void;
    children?: Snippet;
  };

  const {
    screen,
    busy = false,
    selectedGameTitle = null,
    onNavigate = () => undefined,
    onBack = () => undefined,
    children,
  }: Props = $props();

  const workspaceCopy = $derived(resolveWorkspaceCopy(screen, selectedGameTitle));
  const canGoBack = $derived(screen !== 'games');
  const showSettingsButton = $derived(screen !== 'settings');

  function handleOpenSettings(): void {
    onNavigate('settings');
  }

  function resolveWorkspaceCopy(currentScreen: Screen, gameTitle: string | null): WorkspaceCopy {
    switch (currentScreen) {
      case 'details':
        return {
          label: 'Game',
          heading: resolveHeading(gameTitle, 'Selected Game'),
        };

      case 'settings':
        return {
          label: 'Settings',
          heading: 'Settings',
        };

      case 'operations':
        return {
          label: 'Journal',
          heading: resolveHeading(gameTitle, 'Operation Journal'),
        };

      case 'games':
      default:
        return {
          label: 'Library',
          heading: 'Games',
        };
    }
  }

  function resolveHeading(value: string | null, fallback: string): string {
    return value?.trim() ?? fallback;
  }
</script>

<div
  class="
    mx-auto grid min-h-screen max-w-[min(var(--app-shell-width),100%)]
    grid-rows-[auto_minmax(0,1fr)] items-start gap-3 p-4
    max-md:p-3
  "
  aria-busy={busy}
>
  <header
    class="
      sticky top-4 z-20 grid gap-2 self-start
      max-md:top-3
    "
  >
    <Card>
      <CardContent>
        <div
          class="
            flex min-h-0 min-w-0 flex-1 items-center gap-3
            max-md:w-full max-md:items-start
          "
        >
          {#if canGoBack}
            <Button aria-label="Go back" size="icon-sm" variant="outline" onclick={onBack}>
              <ArrowLeftIcon />
            </Button>
          {/if}

          <div class="grid min-w-0 gap-0.5">
            <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
              RenderPilot / {workspaceCopy.label}
            </p>
            <h1
              class="
                truncate text-2xl/tight font-semibold text-foreground
                max-md:text-xl
              "
              title={workspaceCopy.heading}
            >
              {workspaceCopy.heading}
            </h1>
          </div>
        </div>

        <div
          class="
            ml-auto flex shrink-0 items-center gap-2
            max-md:ml-0 max-md:w-full max-md:justify-end
          "
        >
          {#if busy}
            <span class="inline-flex" role="status" aria-live="polite">
              <Badge variant="secondary">
                <span class="size-1.5 rounded-full bg-current" aria-hidden="true"></span>
                Working
              </Badge>
            </span>
          {/if}

          {#if showSettingsButton}
            <Button
              aria-label="Open settings"
              size="icon-sm"
              variant="outline"
              onclick={handleOpenSettings}
            >
              <Settings2Icon />
            </Button>
          {/if}
        </div>
      </CardContent>
    </Card>
  </header>

  <main class="grid min-w-0 gap-4">
    {@render children?.()}
  </main>
</div>
