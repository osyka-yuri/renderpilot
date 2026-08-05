import { zhHans } from '../messages/zh-Hans';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { bindExternalMessages, mergeExternalMessages } from '../messages/external';
import { lumaOverrides } from '../messages/overrides/luma/zh-Hans';
import { LUMA_SOURCE_CATALOG } from '../messages/overrides/luma/schema';
import { NVAPI_SOURCE_CATALOG } from '../messages/overrides/nvapi/contract.generated';
import { nvapiOverrides } from '../messages/overrides/nvapi/zh-Hans';
import type { LocalePack } from './types';

const zhHansPack = {
  locale: 'zh-Hans',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: zhHans,
  externalMessages: mergeExternalMessages(
    bindExternalMessages(LUMA_SOURCE_CATALOG, lumaOverrides),
    bindExternalMessages(NVAPI_SOURCE_CATALOG, nvapiOverrides),
  ),
} as const satisfies LocalePack;

export default zhHansPack;
