<script lang="ts">
  import type { Screen, ScreenHandler } from '@app/navigation/screen';

  import DesktopShell from './DesktopShell.svelte';

  type Props = {
    screen?: Screen;
    selectedGameTitle?: string;
    onNavigate: ScreenHandler;
    onPreload: ScreenHandler;
  };

  const {
    screen = 'games',
    selectedGameTitle = 'Test Game',
    onNavigate,
    onPreload,
  }: Props = $props();

  let currentScreen = $derived(screen);
  let currentGameTitle = $derived(selectedGameTitle);

  function handleNavigate(target: Screen): void {
    currentScreen = target;
    onNavigate(target);
  }
</script>

<DesktopShell
  screen={currentScreen}
  selectedGameTitle={currentGameTitle}
  onNavigate={handleNavigate}
  {onPreload}
>
  <p>Shell content</p>
  <button
    type="button"
    data-test-action="rename-game"
    onclick={() => {
      currentGameTitle = 'Renamed Test Game';
    }}
  >
    Rename game
  </button>
</DesktopShell>
