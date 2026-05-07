declare module '*.svelte' {
  import { SvelteComponent } from 'svelte';
  import { LegacyComponentType } from 'svelte/legacy';

  const Component: LegacyComponentType;
  type Component = SvelteComponent;

  export default Component;
}
