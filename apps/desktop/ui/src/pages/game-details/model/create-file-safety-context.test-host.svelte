<script lang="ts">
  import { onDestroy, untrack } from 'svelte';

  import {
    createFileSafetyContext,
    type CapturedFileSafetyContext,
    type FileSafetyScope,
  } from './create-file-safety-context.svelte';
  import { createMutationConfirmationCoordinator } from './create-mutation-confirmation.svelte';

  let {
    initialGameId,
    initialInstallPath = '/games/test',
  }: { initialGameId: string; initialInstallPath?: string } = $props();
  let gameId = $state<string | null>(untrack(() => initialGameId));
  let installPath = $state<string | null>(untrack(() => initialInstallPath));
  const context = createFileSafetyContext({
    getGameId: () => gameId,
    getInstallPath: () => installPath,
  });
  const confirmation = createMutationConfirmationCoordinator({
    getGameId: () => gameId,
    getInstallPath: () => installPath,
    captureFreshContext: (scope) => context.captureFreshContext(scope),
    isCurrentCapture: (captured) => context.isCurrentCapture(captured),
    invalidatePendingCapture: () => {
      context.invalidatePendingCapture();
    },
  });

  export function replaceGameId(nextGameId: string): void {
    gameId = nextGameId;
  }

  export function replaceInstallPath(nextInstallPath: string): void {
    installPath = nextInstallPath;
  }

  export function requireMutationTokens(scope: FileSafetyScope) {
    return confirmation.requestTokens({ scope });
  }

  export function captureFreshContext(scope: FileSafetyScope) {
    return context.captureFreshContext(scope);
  }

  export function isCurrentCapture(captured: CapturedFileSafetyContext) {
    return context.isCurrentCapture(captured);
  }

  export function resolveMutationConfirmation(accepted: boolean, remember = false): void {
    confirmation.pending?.resolve(accepted, remember);
  }

  export function cancelMutationConfirmation(): void {
    confirmation.cancel();
  }

  export function getMutationConfirmation() {
    return confirmation.pending;
  }

  onDestroy(() => {
    confirmation.destroy();
    context.destroy();
  });
</script>
