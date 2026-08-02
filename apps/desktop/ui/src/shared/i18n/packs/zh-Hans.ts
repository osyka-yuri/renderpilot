import { zhHans } from '../messages/zh-Hans';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/zh-Hans';
import type { LocalePack } from './types';

const zhHansPack = {
  locale: 'zh-Hans',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: zhHans,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default zhHansPack;
