<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';

  import { createPresentedLibraries } from '@shared/graphics';
  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';
  import { Badge } from '@shared/ui';

  const ROOT_CLASS_NAME = 'flex flex-wrap gap-1.5';

  type Props = HTMLAttributes<HTMLDivElement> & {
    libraries?: readonly string[];
  };

  let { libraries = [], class: className = '', ...rest }: Props = $props();

  const presentedLibraries = $derived(createPresentedLibraries(libraries));
  const hasPresentedLibraries = $derived(presentedLibraries.length > 0);
  const rootClassName = $derived(cn(ROOT_CLASS_NAME, className));
</script>

<div {...rest} class={rootClassName}>
  {#if hasPresentedLibraries}
    {#each presentedLibraries as library (library.tag)}
      <Badge variant="outline">
        {library.label}
      </Badge>
    {/each}
  {:else}
    <Badge variant="outline">
      {t('game.libraries.empty')}
    </Badge>
  {/if}
</div>
