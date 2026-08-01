import type { MessageKey, MessageParams } from '@shared/i18n';

export type ToolI18nPrefix = 'gameDetails.luma' | 'gameDetails.renodx';

type ToolMessageSuffixFor<Prefix extends ToolI18nPrefix> =
  Extract<MessageKey, `${Prefix}.${string}`> extends infer Key
    ? Key extends `${Prefix}.${infer Suffix}`
      ? Suffix
      : never
    : never;

type ToolMessageSuffix = Extract<
  ToolMessageSuffixFor<'gameDetails.luma'>,
  ToolMessageSuffixFor<'gameDetails.renodx'>
>;

type ToolMessageKey<Prefix extends ToolI18nPrefix, Suffix extends ToolMessageSuffix> = Extract<
  `${Prefix}.${Suffix}`,
  MessageKey
>;

type SameParams<Left, Right> = [Left] extends [Right]
  ? [Right] extends [Left]
    ? true
    : false
  : false;

type ToolMessageContractError = {
  [Suffix in ToolMessageSuffix]: ToolMessageKey<'gameDetails.luma', Suffix> extends infer LumaKey
    ? ToolMessageKey<'gameDetails.renodx', Suffix> extends infer RenoDxKey
      ? [LumaKey] extends [never]
        ? `missing-luma:${Suffix}`
        : [RenoDxKey] extends [never]
          ? `missing-renodx:${Suffix}`
          : LumaKey extends MessageKey
            ? RenoDxKey extends MessageKey
              ? SameParams<MessageParams<LumaKey>, MessageParams<RenoDxKey>> extends true
                ? never
                : `parameter-mismatch:${Suffix}`
              : never
            : never
      : never
    : never;
}[ToolMessageSuffix];

type VerifiedToolMessageSuffix = [ToolMessageContractError] extends [never]
  ? ToolMessageSuffix
  : never;

/**
 * Builds a catalog key from the suffixes shared by Luma and RenoDX.
 * `VerifiedToolMessageSuffix` becomes `never` if the parameter contracts for
 * any shared suffix diverge, so no call can compile against a broken mirror.
 */
export function toolMessageKey<
  Prefix extends ToolI18nPrefix,
  Suffix extends VerifiedToolMessageSuffix,
>(prefix: Prefix, suffix: Suffix): ToolMessageKey<Prefix, Suffix> {
  // The contract above proves this concatenation is an exact MessageKey.
  return `${prefix}.${suffix}`;
}
