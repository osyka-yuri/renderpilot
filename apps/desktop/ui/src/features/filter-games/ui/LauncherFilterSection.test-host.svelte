<script lang="ts">
  import type { LauncherFilterOption } from '../model/launcher-filter-options';
  import LauncherFilterSection from './LauncherFilterSection.svelte';

  type Props = {
    options: readonly LauncherFilterOption[];
    draftLaunchers: readonly string[];
    draftLauncherOrder: readonly string[];
    onLaunchersChange?: (launchers: readonly string[]) => void;
    onOrderChange?: (order: readonly string[]) => void;
    onKeyboardReorderActiveChange?: (active: boolean) => void;
  };

  let {
    options: initialOptions,
    draftLaunchers: initialDraftLaunchers,
    draftLauncherOrder: initialDraftLauncherOrder,
    onLaunchersChange,
    onOrderChange,
    onKeyboardReorderActiveChange,
  }: Props = $props();

  let options = $state<readonly LauncherFilterOption[]>([]);
  let draftLaunchers = $state<readonly string[]>([]);
  let draftLauncherOrder = $state<readonly string[]>([]);

  $effect.pre(() => {
    options = [...initialOptions];
    draftLaunchers = [...initialDraftLaunchers];
    draftLauncherOrder = [...initialDraftLauncherOrder];
  });

  export function updateOptions(nextOptions: readonly LauncherFilterOption[]): void {
    options = [...nextOptions];
  }

  export function updateDraftLauncherOrder(nextOrder: readonly string[]): void {
    draftLauncherOrder = [...nextOrder];
  }
</script>

<LauncherFilterSection
  {options}
  {draftLaunchers}
  {draftLauncherOrder}
  {onLaunchersChange}
  {onOrderChange}
  {onKeyboardReorderActiveChange}
/>
