import { createMessageRef, t } from './runtime.svelte';
import {
  defineLocalizedCatalog,
  defineLocalizedOverrides,
  defineSourceCatalog,
} from './messages/contract';
import { plural, select } from './messages/model';
import type { PLACEHOLDER_CONTRACT_CASES } from './messages/placeholder-contract-cases';
import type {
  IsValidMessageTemplate,
  MessagePlaceholders,
  ParamsForMessage,
} from './messages/params';

type Equal<Left, Right> = [Left] extends [Right] ? ([Right] extends [Left] ? true : false) : false;
type Assert<Value extends true> = Value;
type PlaceholderContractCase = (typeof PLACEHOLDER_CONTRACT_CASES)[number];
type VerifyPlaceholderContractCase<Case extends PlaceholderContractCase> = Case extends {
  template: infer Template extends string;
  valid: infer Valid extends boolean;
  placeholders: infer Placeholders extends readonly string[];
}
  ? Equal<IsValidMessageTemplate<Template>, Valid> extends true
    ? Valid extends true
      ? Equal<MessagePlaceholders<Template>, Placeholders[number]>
      : Equal<MessagePlaceholders<Template>, never>
    : false
  : false;
export type PlaceholderContractCasesMatch = Assert<
  VerifyPlaceholderContractCase<PlaceholderContractCase> extends true ? true : false
>;

const source = defineSourceCatalog({
  greeting: 'Hello, {name}',
  items: plural('count', {
    one: '{count} item from {source}',
    other: '{count} items from {source}',
  }),
  salutation: select('tone', {
    formal: 'Welcome, {name}',
    casual: 'Hi, {name}',
    other: 'Hello, {name}',
  }),
});
void source;

defineLocalizedCatalog<'ru', typeof source>()({
  greeting: 'Здравствуйте, {name}',
  items: plural('count', {
    one: '{count} предмет из {source}',
    few: '{count} предмета из {source}',
    many: '{count} предметов из {source}',
    other: '{count} предмета из {source}',
  }),
  salutation: select('tone', {
    formal: 'Добро пожаловать, {name}',
    casual: 'Привет, {name}',
    other: 'Здравствуйте, {name}',
  }),
});

defineLocalizedOverrides<'ja', typeof source>()({
  items: plural('count', {
    other: '{count} 個（{source}）',
  }),
});

// Public translation calls infer their complete parameter contract from English.
t('nav.games');
t('game.card.action.detailsAria', { title: 'Halo' });
t('game.dashboard.games', { count: 2 });
createMessageRef('nav.games');
createMessageRef('game.card.action.detailsAria', { title: 'Halo' });

function acceptSyntheticSelectParams(params: ParamsForMessage<typeof source.salutation>): void {
  void params;
}

acceptSyntheticSelectParams({ tone: 'formal', name: 'Ada' });

// @ts-expect-error A parameterized message cannot omit its parameters.
t('game.card.action.detailsAria');
// @ts-expect-error A no-parameter message cannot receive a parameter object.
t('nav.games', {});
// @ts-expect-error Required placeholders cannot be omitted.
t('game.card.action.detailsAria', {});
// @ts-expect-error Undeclared parameters are rejected.
t('game.card.action.detailsAria', { title: 'Halo', unexpected: 'value' });
// @ts-expect-error Interpolation values accept only strings and numbers.
t('game.card.action.detailsAria', { title: true });
// @ts-expect-error A plural discriminator must be numeric.
t('game.dashboard.games', { count: '2' });
// @ts-expect-error Message references preserve the same exact parameter contract.
createMessageRef('game.card.action.detailsAria', { title: 'Halo', unexpected: 'value' });
// @ts-expect-error Static translation accepts only keys from the English contract.
t('backend.unknown');
// @ts-expect-error Select arguments accept only named source cases.
acceptSyntheticSelectParams({ tone: 'unknown', name: 'Ada' });
// @ts-expect-error Select parameters include placeholders used by their branches.
acceptSyntheticSelectParams({ tone: 'formal' });

defineSourceCatalog({
  // @ts-expect-error The English plural source contract is exactly one plus other.
  invalidPlural: plural('count', { one: 'One' }),
});

defineSourceCatalog({
  // @ts-expect-error Every brace in an authored catalog must belong to a valid placeholder.
  invalidTemplate: 'Broken {name',
});

defineSourceCatalog({
  // @ts-expect-error Plural discriminator names use the same ASCII placeholder grammar.
  invalidArgument: plural('item-count', { one: 'One', other: 'Other' }),
});

defineLocalizedOverrides<'de', typeof source>()({
  // @ts-expect-error Localized templates use the same strict brace grammar as the source.
  greeting: 'Hallo, {name',
});

// @ts-expect-error Localized catalogs must contain every source key.
defineLocalizedCatalog<'de', typeof source>()({
  greeting: 'Hallo, {name}',
  items: plural('count', {
    one: '{count} Element aus {source}',
    other: '{count} Elemente aus {source}',
  }),
});

defineLocalizedCatalog<'de', typeof source>()({
  greeting: 'Hallo, {name}',
  items: plural('count', {
    one: '{count} Element aus {source}',
    other: '{count} Elemente aus {source}',
  }),
  salutation: select('tone', {
    formal: 'Willkommen, {name}',
    casual: 'Hallo, {name}',
    other: 'Guten Tag, {name}',
  }),
  // @ts-expect-error Localized catalogs cannot add keys outside the source contract.
  unexpected: 'Unerwartet',
});

defineLocalizedCatalog<'de', typeof source>()({
  greeting: 'Hallo, {name}',
  // @ts-expect-error Localized message tags must match the source tag.
  items: '{count} Elemente aus {source}',
  salutation: select('tone', {
    formal: 'Willkommen, {name}',
    casual: 'Hallo, {name}',
    other: 'Guten Tag, {name}',
  }),
});

defineLocalizedOverrides<'de', typeof source>()({
  // @ts-expect-error Every localized plural branch must preserve source placeholders.
  items: plural('count', {
    one: '{count} Element aus {source}',
    other: '{count} Elemente',
  }),
});

defineLocalizedOverrides<'de', typeof source>()({
  // @ts-expect-error Localized plural arguments must match the source discriminator.
  items: plural('amount', {
    one: '{amount} Element aus {source}',
    other: '{amount} Elemente aus {source}',
  }),
});

defineLocalizedOverrides<'de', typeof source>()({
  // @ts-expect-error A locale can use only its declared CLDR plural categories.
  items: plural('count', {
    one: '{count} Element aus {source}',
    other: '{count} Elemente aus {source}',
    many: '{count} Elemente aus {source}',
  }),
});

defineLocalizedOverrides<'de', typeof source>()({
  // @ts-expect-error Select messages must contain the source cases, including other.
  salutation: select('tone', {
    formal: 'Willkommen, {name}',
    casual: 'Hallo, {name}',
    unknown: 'Unbekannt, {name}',
  }),
});
