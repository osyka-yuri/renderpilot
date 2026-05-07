<script lang="ts">
  import type { ScreenHandler, VoidHandler } from '@shared/utils/callbacks';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import type { Screen } from '@app/routes/screen';

  type WorkspaceCopy = {
    label: string;
    heading: string;
  };

  const noop: VoidHandler = (): void => {
    return;
  };
  const noopNavigate: ScreenHandler = (): void => {
    return;
  };

  export let screen: Screen;
  export let busy = false;
  export let selectedGameTitle: string | null = null;
  export let errorMessage = '';
  export let onNavigate: ScreenHandler = noopNavigate;
  export let onBack: VoidHandler = noop;

  $: workspaceCopy = resolveWorkspaceCopy(screen, selectedGameTitle);
  $: normalizedErrorMessage = errorMessage.trim();
  $: canGoBack = screen !== 'games';
  $: showSettingsButton = screen !== 'settings';
  $: showError = normalizedErrorMessage.length > 0;

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

<div class="shell" aria-busy={busy}>
  <header class="app-header">
    <div class="command-row">
      <div class="nav-group">
        {#if canGoBack}
          <Button aria-label="Go back" iconOnly onclick={onBack}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M14.5 5.5L8 12l6.5 6.5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </Button>
        {/if}

        <div class="title-stack">
          <p class="eyebrow">RenderPilot / {workspaceCopy.label}</p>
          <h1 title={workspaceCopy.heading}>{workspaceCopy.heading}</h1>
        </div>
      </div>

      <div class="action-group">
        {#if busy}
          <span class="status-indicator" role="status" aria-live="polite">
            <Badge pill dot tone="warning">Working</Badge>
          </span>
        {/if}

        {#if showSettingsButton}
          <Button aria-label="Open settings" iconOnly onclick={handleOpenSettings}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M9.6 3.4h4.8l.6 2.3c.5.1 1 .3 1.5.6l2.1-1.2 2.4 4.2-1.8 1.5c.1.5.1 1 .1 1.5s0 1-.1 1.5l1.8 1.5-2.4 4.2-2.1-1.2c-.5.3-1 .5-1.5.6l-.6 2.3H9.6L9 18.3c-.5-.1-1-.3-1.5-.6l-2.1 1.2L3 14.7l1.8-1.5c-.1-.5-.1-1-.1-1.5s0-1 .1-1.5L3 8.7l2.4-4.2 2.1 1.2c.5-.3 1-.5 1.5-.6l.6-2.3z"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linejoin="round"
              />
              <circle
                cx="12"
                cy="12"
                r="2.7"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
              />
            </svg>
          </Button>
        {/if}
      </div>
    </div>

    {#if showError}
      <div class="info-bar" data-tone="danger" role="alert" aria-live="assertive">
        <Badge pill dot tone="danger">Needs attention</Badge>
        <p class="feedback-copy error">{normalizedErrorMessage}</p>
      </div>
    {/if}
  </header>

  <main class="workspace-body">
    <slot />
  </main>
</div>

<style>
  .shell {
    min-height: 100vh;
    max-width: min(var(--app-shell-width), 100%);
    margin: 0 auto;
    padding: var(--space-4);

    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    align-items: start;
    gap: var(--space-3);
  }

  .app-header {
    position: sticky;
    top: var(--space-4);
    z-index: 20;

    display: grid;
    align-self: start;
    gap: var(--space-2);
  }

  .command-row,
  .info-bar {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-layer) 86%, transparent);
    box-shadow: var(--shadow-card);
    backdrop-filter: blur(20px) saturate(120%);
  }

  .command-row {
    min-height: 4rem;
    padding: var(--space-3) var(--space-4);

    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .nav-group {
    min-width: 0;
    min-height: 0;
    flex: 1;

    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .title-stack {
    min-width: 0;

    display: grid;
    gap: 0.12rem;
  }

  .eyebrow,
  h1,
  .feedback-copy {
    margin: 0;
  }

  .eyebrow {
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h1 {
    overflow: hidden;
    color: var(--text-strong);
    font-size: 1.45rem;
    font-weight: 600;
    line-height: 1.15;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-group {
    flex-shrink: 0;
    margin-left: auto;

    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .status-indicator {
    display: inline-flex;
  }

  .info-bar {
    min-height: 2.4rem;
    padding: var(--space-2) var(--space-3);

    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .info-bar[data-tone='danger'] {
    border-color: color-mix(in srgb, var(--danger) 30%, var(--border-subtle));
    background: color-mix(in srgb, var(--danger) 8%, var(--bg-layer));
  }

  .feedback-copy {
    min-width: 0;
    color: var(--text-muted);
    font-size: 0.8125rem;
    line-height: 1.3;
  }

  .feedback-copy.error {
    color: var(--danger);
  }

  .workspace-body {
    min-width: 0;

    display: grid;
    gap: var(--space-4);
  }

  @media (max-width: 720px) {
    .shell {
      padding: var(--space-3);
    }

    .app-header {
      top: var(--space-3);
    }

    .command-row {
      padding: var(--space-3);

      align-items: flex-start;
      flex-wrap: wrap;
    }

    .nav-group {
      width: 100%;
      align-items: flex-start;
    }

    .action-group {
      width: 100%;
      margin-left: 0;
      justify-content: flex-end;
    }

    .info-bar {
      flex-direction: column;
      align-items: flex-start;
      gap: var(--space-2);
    }

    h1 {
      font-size: 1.25rem;
    }
  }
</style>
