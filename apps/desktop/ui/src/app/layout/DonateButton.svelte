<script lang="ts">
  import HeartIcon from '@lucide/svelte/icons/heart';
  import { open } from '@tauri-apps/plugin-shell';
  import { t } from '@shared/i18n';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';

  const DONATE_URL = 'https://boosty.to/osyka.yuri/donate';

  type TriggerProps = {
    onclick?: (event: MouseEvent) => void;
    [key: string]: unknown;
  };

  async function openDonatePage(): Promise<void> {
    try {
      await open(DONATE_URL);
    } catch (error) {
      console.error(error);
    }
  }

  function createClickHandler(props: TriggerProps) {
    return (event: MouseEvent): void => {
      props.onclick?.(event);

      event.preventDefault();

      void openDonatePage();
    };
  }
</script>

<Tooltip>
  <TooltipTrigger>
    {#snippet child({ props }: { props: TriggerProps })}
      <Button
        {...props}
        variant="outline"
        size="icon"
        aria-label={t('nav.donate')}
        onclick={createClickHandler(props)}
      >
        <HeartIcon class="text-red-500" aria-hidden="true" />
      </Button>
    {/snippet}
  </TooltipTrigger>

  <TooltipContent side="bottom" align="center">
    {t('nav.donate')}
  </TooltipContent>
</Tooltip>
