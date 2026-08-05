import { de } from '../messages/de';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { bindExternalMessages, mergeExternalMessages } from '../messages/external';
import { lumaOverrides } from '../messages/overrides/luma/de';
import { LUMA_SOURCE_CATALOG } from '../messages/overrides/luma/schema';
import { NVAPI_SOURCE_CATALOG } from '../messages/overrides/nvapi/contract.generated';
import { nvapiOverrides } from '../messages/overrides/nvapi/de';
import type { LocalePack } from './types';

const dePack = {
  locale: 'de',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: de,
  externalMessages: mergeExternalMessages(
    bindExternalMessages(LUMA_SOURCE_CATALOG, lumaOverrides),
    bindExternalMessages(NVAPI_SOURCE_CATALOG, nvapiOverrides),
  ),
} as const satisfies LocalePack;

export default dePack;
