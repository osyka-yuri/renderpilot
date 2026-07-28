import { es } from '../messages/es';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/es';
import type { LocalePack } from './types';

const esPack = {
  locale: 'es',
  messages: es,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default esPack;
