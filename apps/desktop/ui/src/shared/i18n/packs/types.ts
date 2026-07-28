import type { Locale } from '../locale';
import type { MessageKey } from '../messages/en';
import type { MessageOverrides, MessageValue } from '../messages/types';

export type LocalePack = Readonly<{
  locale: Locale;
  messages: Readonly<Record<MessageKey, MessageValue>>;
  dynamicCatalogs: readonly MessageOverrides[];
}>;

export type LocaleLoader = () => Promise<LocalePack>;
