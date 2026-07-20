<script lang="ts">
  import CopyIcon from '@lucide/svelte/icons/copy';
  import InfoIcon from '@lucide/svelte/icons/info';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
    Button,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import { t } from '@shared/i18n';

  import { createCopyResetTimer, copyWithFeedback } from '../model/copy-feedback';
  import {
    hasKnownLaunchArgsInstructions,
    includesDx11LaunchArg,
    launchArgsInstructionKey,
  } from '../model/launch-args';

  type Props = {
    launchArgs: string[];
    /** The game's launcher (`Launcher` wire value, e.g. `"Steam"`, `"Gog"`, `"Epic"`, …). */
    launcher: string;
  };

  const { launchArgs, launcher }: Props = $props();

  const argsText = $derived(launchArgs.join(' '));
  const includesDx11 = $derived(includesDx11LaunchArg(launchArgs));
  const instructions = $derived(t(launchArgsInstructionKey(launcher)));
  const hasKnownInstructions = $derived(hasKnownLaunchArgsInstructions(launcher));
  const title = $derived(
    includesDx11
      ? t('gameDetails.luma.launchArgs.dx11Title')
      : t('gameDetails.luma.launchArgs.title'),
  );

  type CopyStatus = 'idle' | 'copied';
  let copyStatus = $state<CopyStatus>('idle');
  const resetTimer = createCopyResetTimer(() => {
    copyStatus = 'idle';
  });

  const copyLabel = $derived(
    copyStatus === 'copied'
      ? t('gameDetails.luma.launchArgs.copied')
      : t('gameDetails.luma.launchArgs.copy'),
  );

  $effect(() => () => {
    resetTimer.dispose();
  });

  async function copyArgs(): Promise<void> {
    const ok = await copyWithFeedback(argsText, {
      copied: 'gameDetails.luma.launchArgs.copied',
      copyFailed: 'gameDetails.luma.launchArgs.copyFailed',
    });
    if (ok) {
      copyStatus = 'copied';
      resetTimer.arm();
    }
  }
</script>

{#if launchArgs.length > 0}
  <Alert variant="default" size="sm">
    <InfoIcon aria-hidden="true" />
    <AlertTitle>{title}</AlertTitle>
    <AlertDescription>
      <ol class="mt-2 list-decimal space-y-2 pl-4">
        <li class="space-y-1.5">
          <span>{t('gameDetails.luma.launchArgs.copyStep')}</span>
          <div class="relative inline-flex max-w-full items-center">
            <code class="overflow-x-auto rounded-sm bg-muted py-1 pr-8 pl-1.5 text-xs"
              >{argsText}</code
            >
            <Tooltip>
              <TooltipTrigger type="button" onclick={copyArgs} aria-label={copyLabel}>
                {#snippet child({ props })}
                  <Button {...props} variant="ghost" size="icon" class="absolute right-0.5 size-6">
                    <CopyIcon class="size-3" aria-hidden="true" />
                  </Button>
                {/snippet}
              </TooltipTrigger>
              <TooltipContent>{t('gameDetails.luma.launchArgs.copy')}</TooltipContent>
            </Tooltip>
          </div>
        </li>
        <li class="space-y-1">
          <p>{instructions}</p>
          {#if hasKnownInstructions}
            <p class="text-xs text-muted-foreground">
              {t('gameDetails.luma.launchArgs.instructions.other')}
            </p>
          {/if}
        </li>
      </ol>
      <span class="sr-only" aria-live="polite">{copyStatus === 'copied' ? copyLabel : ''}</span>
    </AlertDescription>
  </Alert>
{/if}
