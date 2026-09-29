<script lang="ts">
  import { SegmentedControl, SegmentedControlItem } from './index';

  type Props = {
    rootDisabled?: boolean;
  };

  let { rootDisabled = false }: Props = $props();
  let value = $state('cards');
  const accessibleName = 'Catalog view';

  function onValueChange(nextValue: string): void {
    value = nextValue;
  }
</script>

<SegmentedControl aria-label={accessibleName} {value} {onValueChange} disabled={rootDisabled}>
  <SegmentedControlItem value="cards">Cards</SegmentedControlItem>
  <SegmentedControlItem value="list">List</SegmentedControlItem>
  <SegmentedControlItem value="disabled" disabled>
    <span>Disabled</span>
  </SegmentedControlItem>
</SegmentedControl>

<output>{value}</output>
