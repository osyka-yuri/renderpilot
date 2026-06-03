<script lang="ts">
  import HeartIcon from '@lucide/svelte/icons/heart';
  import { open } from '@tauri-apps/plugin-shell';
  import { t } from '@shared/i18n';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
</script>

<Tooltip>
  <TooltipTrigger>
    {#snippet child({ props })}
      <Button
        variant="outline"
        size="icon"
        aria-label={t('nav.donate')}
        {...props}
        onclick={(e: MouseEvent) => {
          if (typeof props.onclick === 'function') {
            props.onclick(e);
          }
          e.preventDefault();
          open("https://boosty.to/osyka.yuri/donate").catch(console.error);
        }}
      >
        <HeartIcon class="text-red-500" aria-hidden="true" />
      </Button>
    {/snippet}
  </TooltipTrigger>
  <TooltipContent side="bottom" align="center">
    {t('nav.donate')}
  </TooltipContent>
</Tooltip>
