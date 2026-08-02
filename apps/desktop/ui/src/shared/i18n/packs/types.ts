import type { Locale } from '../locale-model';
import type { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import type { MessageOverrides, MessageValue } from '../messages/model';

export type LocalePack = Readonly<{
  locale: Locale;
  contractVersion: typeof MESSAGE_CONTRACT_VERSION;
  messages: Readonly<Record<string, MessageValue>>;
  dynamicCatalogs: readonly MessageOverrides[];
}>;

export type LocaleLoader = () => Promise<LocalePack>;
