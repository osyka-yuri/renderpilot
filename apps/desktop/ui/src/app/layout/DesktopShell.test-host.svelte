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
</script>

<DesktopShell {screen} {selectedGameTitle} {onNavigate} {onPreload}>
  <p>Shell content</p>
</DesktopShell>
