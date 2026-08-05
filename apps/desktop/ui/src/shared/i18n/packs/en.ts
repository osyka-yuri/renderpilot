import { en } from '../messages/en';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import type { LocalePack } from './types';

export const enPack = {
  locale: 'en',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: en,
  externalMessages: {},
} as const satisfies LocalePack;

export default enPack;
