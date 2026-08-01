import { zh } from '../messages/zh';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/zh';
import type { LocalePack } from './types';

const zhPack = {
  locale: 'zh',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: zh,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default zhPack;
