import { de } from '../messages/de';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/de';
import type { LocalePack } from './types';

const dePack = {
  locale: 'de',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: de,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default dePack;
