import { analyzeMessageTemplate as analyzeSharedMessageTemplate } from '../../ui/src/shared/i18n/messages/template.ts';
import {
  ExternalContractValidationError,
  projectSupportedNvapiCatalog,
  validateLumaContract as validateLumaContractCore,
} from '../external-contract-core.mjs';

function fail(message) {
  throw new Error(`i18n contract generation failed: ${message}`);
}

export function analyzeMessageTemplate(template) {
  const analysis = analyzeSharedMessageTemplate(template);
  return { valid: analysis.valid, placeholders: analysis.placeholders };
}

function messagePlaceholders(template, context) {
  const analysis = analyzeMessageTemplate(template);
  if (!analysis.valid) {
    fail(`${context} contains invalid placeholder syntax`);
  }
  return analysis.placeholders.toSorted();
}

function assertPlaceholderName(value, context) {
  const analysis = analyzeMessageTemplate(`{${value}}`);
  if (!analysis.valid || analysis.placeholders.length !== 1 || analysis.placeholders[0] !== value) {
    fail(`${context} has invalid placeholder name ${JSON.stringify(value)}`);
  }
}

function duplicateValues(values) {
  return values.filter((value, index) => values.indexOf(value) !== index);
}

export function sortRecord(record) {
  return Object.fromEntries(
    Object.entries(record).sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0)),
  );
}

const MACHINE_CODE_PATTERN = /^[a-z][a-z0-9_]*$/;
const MAX_MACHINE_CODE_LENGTH = 64;
const RUST_VARIANT_PATTERN = /^[A-Z][A-Za-z0-9]*$/;
const WARNING_PARAMETER_TYPES = new Set(['positive_integer', 'non_blank_string']);
const PLURAL_CATEGORIES = new Set(['zero', 'one', 'two', 'few', 'many', 'other']);

function assertUniqueByCode(entries, context) {
  if (!Array.isArray(entries)) {
    fail(`${context} must be an array`);
  }

  const seen = new Set();
  for (const entry of entries) {
    if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
      fail(`${context} entries must be objects`);
    }
    assertMachineCode(entry.code, `${context} code`);
    if (seen.has(entry.code)) {
      fail(`${context} contains duplicate code ${entry.code}`);
    }
    seen.add(entry.code);
  }
}

function assertAllowedKeys(value, allowedKeys, context) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail(`${context} must be an object`);
  }
  const unknownKeys = Object.keys(value).filter((key) => !allowedKeys.includes(key));
  if (unknownKeys.length > 0) {
    fail(`${context} contains unknown field ${JSON.stringify(unknownKeys[0])}`);
  }
}

function assertKeys(value, requiredKeys, optionalKeys, context) {
  assertAllowedKeys(value, [...requiredKeys, ...optionalKeys], context);
  const missingKeys = requiredKeys.filter((key) => !Object.hasOwn(value, key));
  if (missingKeys.length > 0) {
    fail(`${context} is missing field ${JSON.stringify(missingKeys[0])}`);
  }
}

function assertExactKeys(value, keys, context) {
  assertKeys(value, keys, [], context);
}

function isMachineCode(value) {
  return (
    typeof value === 'string' &&
    value.length <= MAX_MACHINE_CODE_LENGTH &&
    MACHINE_CODE_PATTERN.test(value)
  );
}

function assertMachineCode(value, context) {
  if (!isMachineCode(value)) {
    fail(`${context} has invalid machine code ${JSON.stringify(value)}`);
  }
}

function assertCatalogMessage(english, key, context, expectedParameters) {
  if (typeof key !== 'string' || !Object.hasOwn(english, key)) {
    fail(`${context} references missing English message ${JSON.stringify(key)}`);
  }

  const message = english[key];
  const actualParameters = new Set(
    message.kind === 'string'
      ? message.placeholders
      : [message.argument, ...Object.values(message.branches).flat()],
  );
  const expected = new Set(expectedParameters);
  if (actualParameters.size !== expected.size || !actualParameters.isSubsetOf(expected)) {
    fail(
      `${context} parameters ${JSON.stringify(Array.from(expected).sort())} do not match English message ${key} parameters ${JSON.stringify(Array.from(actualParameters).sort())}`,
    );
  }
}

/** Validates the presentation-neutral desktop command-error manifest. */
export function validateDesktopCommandErrorContract(value, english) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail('desktop command error contract must be an object');
  }
  assertExactKeys(
    value,
    ['schemaVersion', 'commandErrors', 'suggestedActions'],
    'desktop command error contract',
  );
  if (value.schemaVersion !== 1) {
    fail(
      `desktop command error contract has unsupported schemaVersion ${JSON.stringify(value.schemaVersion)}`,
    );
  }

  assertUniqueByCode(value.commandErrors, 'desktop commandErrors');
  assertUniqueByCode(value.suggestedActions, 'desktop suggestedActions');

  const actionCodes = new Set(value.suggestedActions.map(({ code }) => code));
  const actions = {};
  for (const action of value.suggestedActions) {
    assertExactKeys(action, ['code', 'messageKey'], `suggested action ${action.code}`);
    assertCatalogMessage(english, action.messageKey, `suggested action ${action.code}`, []);
    actions[action.code] = { messageKey: action.messageKey };
  }

  const commandErrors = {};
  const rustVariants = new Set();
  for (const error of value.commandErrors) {
    assertKeys(
      error,
      ['code', 'rustVariant', 'messageKey', 'severity', 'actions'],
      ['reasonCodes', 'recoveryBundlePath'],
      `desktop command error ${error.code}`,
    );
    if (typeof error.rustVariant !== 'string' || !RUST_VARIANT_PATTERN.test(error.rustVariant)) {
      fail(
        `desktop command error ${error.code} has invalid rustVariant ${JSON.stringify(error.rustVariant)}`,
      );
    }
    if (rustVariants.has(error.rustVariant)) {
      fail(`desktop commandErrors contains duplicate rustVariant ${error.rustVariant}`);
    }
    rustVariants.add(error.rustVariant);
    if (error.severity !== 'warning' && error.severity !== 'error') {
      fail(
        `desktop command error ${error.code} has invalid severity ${JSON.stringify(error.severity)}`,
      );
    }
    if (!Array.isArray(error.actions) || error.actions.some((code) => !actionCodes.has(code))) {
      fail(`desktop command error ${error.code} references an unknown suggested action`);
    }
    if (new Set(error.actions).size !== error.actions.length) {
      fail(`desktop command error ${error.code} contains duplicate suggested actions`);
    }
    assertCatalogMessage(english, error.messageKey, `desktop command error ${error.code}`, []);

    const reasonCodes = error.reasonCodes ?? [];
    if (
      !Array.isArray(reasonCodes) ||
      reasonCodes.some((code) => !isMachineCode(code)) ||
      new Set(reasonCodes).size !== reasonCodes.length
    ) {
      fail(`desktop command error ${error.code} has invalid or duplicate reasonCodes`);
    }
    if (error.recoveryBundlePath !== undefined && error.recoveryBundlePath !== true) {
      fail(`desktop command error ${error.code} recoveryBundlePath must be true when present`);
    }
    if (error.recoveryBundlePath === true && error.severity !== 'error') {
      fail(`desktop command error ${error.code} exposes a recovery path but is not an error`);
    }

    commandErrors[error.code] = {
      rustVariant: error.rustVariant,
      messageKey: error.messageKey,
      severity: error.severity,
      actions: [...error.actions],
      reasonCodes: [...reasonCodes],
      recoveryBundlePath: error.recoveryBundlePath === true,
    };
  }

  return {
    schemaVersion: 1,
    commandErrors: sortRecord(commandErrors),
    suggestedActions: sortRecord(actions),
  };
}

/** Validates the feature-local add-game warning manifest. */
export function validateAddGameWarningContract(value, english) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail('add-game warning contract must be an object');
  }
  assertExactKeys(value, ['schemaVersion', 'addGameWarnings'], 'add-game warning contract');
  if (value.schemaVersion !== 1) {
    fail(
      `add-game warning contract has unsupported schemaVersion ${JSON.stringify(value.schemaVersion)}`,
    );
  }

  assertUniqueByCode(value.addGameWarnings, 'add-game warnings');
  const addGameWarnings = {};
  for (const warning of value.addGameWarnings) {
    assertExactKeys(
      warning,
      ['code', 'messageKey', 'parameters'],
      `add-game warning ${warning.code}`,
    );
    if (
      warning.parameters === null ||
      typeof warning.parameters !== 'object' ||
      Array.isArray(warning.parameters)
    ) {
      fail(`add-game warning ${warning.code} parameters must be an object`);
    }
    for (const [name, type] of Object.entries(warning.parameters)) {
      assertMachineCode(name, `add-game warning ${warning.code} parameter`);
      assertPlaceholderName(name, `add-game warning ${warning.code} parameter`);
      if (!WARNING_PARAMETER_TYPES.has(type)) {
        fail(
          `add-game warning ${warning.code} parameter ${name} has invalid type ${JSON.stringify(type)}`,
        );
      }
    }
    assertCatalogMessage(
      english,
      warning.messageKey,
      `add-game warning ${warning.code}`,
      Object.keys(warning.parameters),
    );
    addGameWarnings[warning.code] = {
      messageKey: warning.messageKey,
      parameters: sortRecord(warning.parameters),
    };
  }

  return {
    schemaVersion: 1,
    addGameWarnings: sortRecord(addGameWarnings),
  };
}

export function validateEnglishContract(entries) {
  const messages = {};
  for (const [key, value] of entries) {
    if (Object.hasOwn(messages, key)) {
      fail(`English catalog contains duplicate key ${key}`);
    }
    if (value.kind === 'string') {
      messages[key] = {
        kind: 'string',
        placeholders: messagePlaceholders(value.template, `English message ${key}`),
      };
      continue;
    }
    if (value.helper !== 'plural' && value.helper !== 'select') {
      fail(`English message ${key} uses unsupported helper ${value.helper}()`);
    }

    assertPlaceholderName(value.argument, `English message ${key} argument`);
    const branchNames = value.branches.map(([name]) => name);
    if (duplicateValues(branchNames).length > 0) {
      fail(`English message ${key} contains duplicate branches`);
    }
    const sortedBranchNames = branchNames.toSorted();
    if (value.helper === 'plural' && sortedBranchNames.join(',') !== 'one,other') {
      fail(`English message ${key} plural branches must be exactly one and other`);
    }
    if (value.helper === 'select' && (!branchNames.includes('other') || branchNames.length < 2)) {
      fail(`English message ${key} select branches must include other and at least one named case`);
    }

    messages[key] = {
      kind: value.helper,
      argument: value.argument,
      branches: sortRecord(
        Object.fromEntries(
          value.branches.map(([name, template]) => [
            name,
            messagePlaceholders(template, `English message ${key}.${name}`),
          ]),
        ),
      ),
    };
  }
  return sortRecord(messages);
}

export function validatePluralCategories(entries) {
  const categories = {};
  for (const [locale, values] of entries) {
    if (Object.hasOwn(categories, locale)) {
      fail(`PLURAL_CATEGORIES contains duplicate locale ${locale}`);
    }
    if (values.length === 0) {
      fail(`PLURAL_CATEGORIES.${locale} must be a non-empty literal array`);
    }
    const uniqueCategories = new Set(values);
    if (
      uniqueCategories.size !== values.length ||
      !uniqueCategories.has('other') ||
      !uniqueCategories.isSubsetOf(PLURAL_CATEGORIES)
    ) {
      fail(`PLURAL_CATEGORIES.${locale} contains invalid or duplicate categories`);
    }
    categories[locale] = values;
  }
  return sortRecord(categories);
}

export function validateLumaContract(value) {
  try {
    return validateLumaContractCore(value);
  } catch (cause) {
    if (cause instanceof ExternalContractValidationError) {
      fail(cause.message);
    }
    throw cause;
  }
}

function nonEmptyString(value, context) {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`${context} must be a non-empty string`);
  }
  return value;
}

export function validateNvapiContract(value) {
  try {
    return projectSupportedNvapiCatalog(value);
  } catch (cause) {
    if (cause instanceof ExternalContractValidationError) {
      fail(cause.message);
    }
    throw cause;
  }
}

export function validateExternalCatalogBoundaries(english, luma, nvapi) {
  const owners = [
    ['English', Object.keys(english)],
    ['Luma', Object.keys(luma.sourceCatalog)],
    ['NVAPI', Object.keys(nvapi.sourceCatalog)],
  ];
  const seen = new Map();
  for (const [owner, keys] of owners) {
    for (const key of keys) {
      const previous = seen.get(key);
      if (previous !== undefined) {
        fail(`message key ${key} is shared by ${previous} and ${owner}`);
      }
      seen.set(key, owner);
    }
  }
}

const NON_ENGLISH_LOCALES = ['ru', 'de', 'es', 'fr', 'ja', 'zh-Hans', 'zh-Hant'];
const NVIDIA_FAMILY_TERM_KEYS = [
  'superResolution',
  'frameGeneration',
  'multiFrameGeneration',
  'rayReconstruction',
];
const REQUIRED_SCRIPT_VALUES = new Set([null, 'Cyrillic', 'Japanese', 'Han']);
const LAUNCHER_KEYS = ['steam', 'gog', 'epic', 'ea', 'ubisoft'];

function assertUniqueNonEmptyStrings(values, context) {
  if (
    !Array.isArray(values) ||
    values.length === 0 ||
    values.some((entry) => typeof entry !== 'string' || entry.trim() === '') ||
    new Set(values).size !== values.length
  ) {
    fail(`${context} must contain unique non-empty strings`);
  }
}

export function validateEditorialPolicy(value, { english, nvapi }) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    fail('editorial policy must be an object');
  }
  assertExactKeys(
    value,
    [
      'schemaVersion',
      'nvidiaFamilyTerms',
      'nvidiaSources',
      'launcherProductNames',
      'protectedTokens',
      'technicalOnlyStaticKeys',
      'nvapiVerbatimValues',
      'nvapiSemanticTranslations',
      'localeTypography',
      'chineseScriptRules',
    ],
    'editorial policy',
  );
  if (value.schemaVersion !== 2) {
    fail(`editorial policy has unsupported schemaVersion ${JSON.stringify(value.schemaVersion)}`);
  }

  for (const field of [
    'nvidiaFamilyTerms',
    'nvidiaSources',
    'nvapiSemanticTranslations',
    'localeTypography',
  ]) {
    if (!value[field] || typeof value[field] !== 'object' || Array.isArray(value[field])) {
      fail(`editorial policy ${field} must be an object`);
    }
    assertExactKeys(value[field], NON_ENGLISH_LOCALES, `editorial policy ${field}`);
  }
  assertExactKeys(
    value.launcherProductNames,
    LAUNCHER_KEYS,
    'editorial policy launcherProductNames',
  );
  for (const [launcher, productName] of Object.entries(value.launcherProductNames)) {
    nonEmptyString(productName, `editorial policy launcher product ${launcher}`);
  }
  for (const locale of NON_ENGLISH_LOCALES) {
    const terms = value.nvidiaFamilyTerms[locale];
    assertExactKeys(terms, NVIDIA_FAMILY_TERM_KEYS, `editorial policy terms for ${locale}`);
    for (const [key, term] of Object.entries(terms)) {
      nonEmptyString(term, `editorial policy ${locale}.${key}`);
    }
    if (new Set(Object.values(terms)).size !== NVIDIA_FAMILY_TERM_KEYS.length) {
      fail(`editorial policy NVIDIA family terms for ${locale} must be distinct`);
    }
    const nvidiaSource = nonEmptyString(
      value.nvidiaSources[locale],
      `editorial policy ${locale} NVIDIA source`,
    );
    let sourceUrl;
    try {
      sourceUrl = new URL(nvidiaSource);
    } catch {
      fail(`editorial policy ${locale} NVIDIA source must be a valid URL`);
    }
    if (
      sourceUrl.protocol !== 'https:' ||
      (sourceUrl.hostname !== 'www.nvidia.cn' && !sourceUrl.hostname.endsWith('.nvidia.com'))
    ) {
      fail(`editorial policy ${locale} NVIDIA source must use an official HTTPS URL`);
    }
    const typography = value.localeTypography[locale];
    assertExactKeys(
      typography,
      [
        'quotationMarks',
        'forbiddenQuoteMarks',
        'forbiddenPunctuation',
        'sentenceTerminator',
        'requiredScript',
      ],
      `editorial policy typography for ${locale}`,
    );
    assertExactKeys(
      typography.quotationMarks,
      ['open', 'close', 'innerSpacing'],
      `editorial policy quotation marks for ${locale}`,
    );
    const openQuote = nonEmptyString(
      typography.quotationMarks.open,
      `editorial policy ${locale} opening quotation mark`,
    );
    const closeQuote = nonEmptyString(
      typography.quotationMarks.close,
      `editorial policy ${locale} closing quotation mark`,
    );
    if (openQuote === closeQuote || typeof typography.quotationMarks.innerSpacing !== 'boolean') {
      fail(`editorial policy ${locale} quotation marks are invalid`);
    }
    assertUniqueNonEmptyStrings(
      typography.forbiddenQuoteMarks,
      `editorial policy ${locale} forbiddenQuoteMarks`,
    );
    if (
      typography.forbiddenQuoteMarks.includes(openQuote) ||
      typography.forbiddenQuoteMarks.includes(closeQuote)
    ) {
      fail(`editorial policy ${locale} forbids its approved quotation marks`);
    }
    assertUniqueNonEmptyStrings(
      typography.forbiddenPunctuation,
      `editorial policy ${locale} forbiddenPunctuation`,
    );
    nonEmptyString(typography.sentenceTerminator, `editorial policy ${locale} sentenceTerminator`);
    if (!REQUIRED_SCRIPT_VALUES.has(typography.requiredScript)) {
      fail(`editorial policy ${locale} has invalid requiredScript`);
    }
  }

  for (const [field, values] of [
    ['protectedTokens', value.protectedTokens],
    ['technicalOnlyStaticKeys', value.technicalOnlyStaticKeys],
    ['nvapiVerbatimValues', value.nvapiVerbatimValues],
  ]) {
    assertUniqueNonEmptyStrings(values, `editorial policy ${field}`);
  }

  for (const key of value.technicalOnlyStaticKeys) {
    if (!Object.hasOwn(english, key)) {
      fail(`editorial policy technical-only static key is not in the English catalog: ${key}`);
    }
  }
  for (const launcher of LAUNCHER_KEYS) {
    const key = `gameDetails.luma.launchArgs.instructions.${launcher}`;
    if (!Object.hasOwn(english, key)) {
      fail(`editorial policy launcher key is not in the English catalog: ${key}`);
    }
  }

  const nvapiSources = new Set(Object.values(nvapi.sourceCatalog));
  for (const source of value.nvapiVerbatimValues) {
    if (!nvapiSources.has(source)) {
      fail(`editorial policy NVAPI verbatim value is not in the source catalog: ${source}`);
    }
  }

  const semanticSources = Object.keys(value.nvapiSemanticTranslations[NON_ENGLISH_LOCALES[0]]);
  assertUniqueNonEmptyStrings(semanticSources, 'editorial policy NVAPI semantic sources');
  for (const source of semanticSources) {
    if (!nvapiSources.has(source)) {
      fail(`editorial policy NVAPI semantic source is not in the source catalog: ${source}`);
    }
    if (value.nvapiVerbatimValues.includes(source)) {
      fail(`editorial policy NVAPI semantic source is also marked verbatim: ${source}`);
    }
  }
  for (const locale of NON_ENGLISH_LOCALES) {
    const translations = value.nvapiSemanticTranslations[locale];
    assertExactKeys(
      translations,
      semanticSources,
      `editorial policy NVAPI semantic translations for ${locale}`,
    );
    const localizedValues = Object.values(translations);
    for (const [source, translation] of Object.entries(translations)) {
      nonEmptyString(translation, `editorial policy ${locale} NVAPI translation for ${source}`);
    }
    if (new Set(localizedValues).size !== localizedValues.length) {
      fail(`editorial policy NVAPI semantic translations for ${locale} must be distinct`);
    }
  }

  assertExactKeys(
    value.chineseScriptRules,
    ['zh-Hans', 'zh-Hant'],
    'editorial policy chineseScriptRules',
  );
  for (const locale of ['zh-Hans', 'zh-Hant']) {
    const rule = value.chineseScriptRules[locale];
    assertExactKeys(rule, ['forbiddenTerms'], `editorial policy ${locale} script rules`);
    assertUniqueNonEmptyStrings(rule.forbiddenTerms, `editorial policy ${locale} forbiddenTerms`);
  }

  return value;
}
