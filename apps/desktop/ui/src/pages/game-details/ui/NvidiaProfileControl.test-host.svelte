<script lang="ts">
  import NvidiaProfileControl from './NvidiaProfileControl.svelte';
  import type { NvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';

  type Props = {
    gameId: string;
    mode: 'settings' | 'recovery';
    profile: NvapiProfileContext;
    onOpenGameDetails: (gameId: string) => void | Promise<void>;
    onRecoveryDeleteComplete: () => void;
    canCreate: boolean;
  };

  let { gameId: initialGameId, ...props }: Props = $props();
  let gameId = $state(getInitialGameId());

  function getInitialGameId(): string {
    return initialGameId;
  }

  export function setGameId(nextGameId: string): void {
    gameId = nextGameId;
  }
</script>

<NvidiaProfileControl {...props} {gameId} />
