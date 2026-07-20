<script lang="ts">
  import CopyIcon from '@lucide/svelte/icons/copy';
  import FileCode2Icon from '@lucide/svelte/icons/file-code-2';
  import Settings2Icon from '@lucide/svelte/icons/settings-2';
  import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
  import TerminalIcon from '@lucide/svelte/icons/terminal';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import WrenchIcon from '@lucide/svelte/icons/wrench';
  import { t, translateKey, type MessageKey } from '@shared/i18n';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
    Button,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';

  import { createCopyResetTimer, copyWithFeedback } from '../model/copy-feedback';
  import type { LumaGuidance, LumaGuidanceKind } from '../model/types';

  type Props = { guidance: LumaGuidance[] };

  const { guidance }: Props = $props();

  const TITLE_KEYS = {
    game_setting: 'gameDetails.luma.guidance.gameSetting',
    engine_ini: 'gameDetails.luma.guidance.engineIni',
    launch_argument: 'gameDetails.luma.guidance.launchArgument',
    warning: 'gameDetails.luma.guidance.warning',
    compatibility: 'gameDetails.luma.guidance.compatibility',
    external_tool: 'gameDetails.luma.guidance.externalTool',
  } as const satisfies Record<LumaGuidanceKind, MessageKey>;

  const ICONS = {
    game_setting: Settings2Icon,
    engine_ini: FileCode2Icon,
    launch_argument: TerminalIcon,
    warning: TriangleAlertIcon,
    compatibility: ShieldAlertIcon,
    external_tool: WrenchIcon,
  } as const;

  let copiedId = $state<string | null>(null);
  const resetTimer = createCopyResetTimer(() => {
    copiedId = null;
  });

  $effect(() => () => {
    resetTimer.dispose();
  });

  function textFor(item: LumaGuidance): string {
    return translateKey(item.id, item.fallback_text);
  }

  async function copy(item: LumaGuidance): Promise<void> {
    if (!item.code) {
      return;
    }
    const ok = await copyWithFeedback(item.code, {
      copied: 'gameDetails.luma.guidance.copied',
      copyFailed: 'gameDetails.luma.guidance.copyFailed',
    });
    if (ok) {
      copiedId = item.id;
      resetTimer.arm();
    }
  }
</script>

{#each guidance as item (item.id)}
  {@const Icon = ICONS[item.kind]}
  <Alert variant={item.kind === 'warning' ? 'warning' : 'default'} size="sm">
    <Icon aria-hidden="true" />
    {#if item.kind !== 'warning'}
      <AlertTitle>{t(TITLE_KEYS[item.kind])}</AlertTitle>
    {/if}
    <AlertDescription>
      <span>{textFor(item)}</span>
      {#if item.code}
        <div class="relative min-w-0">
          <pre class="overflow-x-auto rounded-sm bg-muted p-2 pr-10 text-xs"><code>{item.code}</code
            ></pre>
          <Tooltip>
            <TooltipTrigger
              type="button"
              onclick={() => copy(item)}
              aria-label={copiedId === item.id
                ? t('gameDetails.luma.guidance.copied')
                : t('gameDetails.luma.guidance.copy')}
            >
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="ghost"
                  size="icon"
                  class="absolute top-1 right-1 size-6"
                >
                  <CopyIcon class="size-3" aria-hidden="true" />
                </Button>
              {/snippet}
            </TooltipTrigger>
            <TooltipContent>{t('gameDetails.luma.guidance.copy')}</TooltipContent>
          </Tooltip>
        </div>
      {/if}
    </AlertDescription>
  </Alert>
{/each}

<span class="sr-only" aria-live="polite">
  {copiedId === null ? '' : t('gameDetails.luma.guidance.copied')}
</span>
