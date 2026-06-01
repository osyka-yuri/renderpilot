import Root from './alert.svelte';
import Description from './alert-description.svelte';
import Title from './alert-title.svelte';
import { alertVariants } from './alert.variants';
import type { AlertVariant, AlertSize } from './alert.types';

export {
  Root,
  Description,
  Title,
  alertVariants,
  type AlertVariant,
  type AlertSize,
  //
  Root as Alert,
  Description as AlertDescription,
  Title as AlertTitle,
};
