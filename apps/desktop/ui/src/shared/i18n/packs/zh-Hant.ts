import { zhHant } from '../messages/zh-Hant';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/zh-Hant';
import type { LocalePack } from './types';

const zhHantPack = {
  locale: 'zh-Hant',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: zhHant,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default zhHantPack;
