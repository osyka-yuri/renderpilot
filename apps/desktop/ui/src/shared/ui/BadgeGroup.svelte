<script lang="ts">
  import { cx } from '@shared/utils/cx';

  export let align: 'start' | 'end' = 'start';

  let className = '';
  export { className as class };

  $: classes = cx('badge-group', align === 'end' && 'badge-group--end', className);
</script>

<div {...$$restProps} class={classes}>
  <slot />
</div>

<style>
  .badge-group {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: flex-start;
    align-content: flex-start;
    justify-content: flex-start;
  }

  .badge-group--end {
    justify-content: flex-end;
  }
</style>
