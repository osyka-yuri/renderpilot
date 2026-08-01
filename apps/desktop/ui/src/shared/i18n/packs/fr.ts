import { fr } from '../messages/fr';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/fr';
import type { LocalePack } from './types';

const frPack = {
  locale: 'fr',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: fr,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default frPack;
