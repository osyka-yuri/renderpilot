import { fr } from '../messages/fr';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/fr';
import type { LocalePack } from './types';

const frPack = {
  locale: 'fr',
  messages: fr,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default frPack;
