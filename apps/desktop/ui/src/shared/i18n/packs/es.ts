import { es } from '../messages/es';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/es';
import type { LocalePack } from './types';

const esPack = {
  locale: 'es',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: es,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default esPack;
