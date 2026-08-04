import { analyzeMessageTemplate as analyzeSharedMessageTemplate } from '../../ui/src/shared/i18n/messages/template.ts';

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

function assertExactKeys(value, allowedKeys, context) {
  const unknownKeys = Object.keys(value).filter((key) => !allowedKeys.includes(key));
  if (unknownKeys.length > 0) {
    fail(`${context} contains unknown field ${JSON.stringify(unknownKeys[0])}`);
  }
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
    assertExactKeys(
      error,
      [
        'code',
        'rustVariant',
        'messageKey',
        'severity',
        'actions',
        'reasonCodes',
        'recoveryBundlePath',
      ],
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

function assertIdentifier(value, context) {
  if (!/^[a-z][A-Za-z0-9]*$/.test(value)) {
    fail(`${context} has invalid identifier ${JSON.stringify(value)}`);
  }
}

function assertMessageId(value, prefix, context) {
  if (
    typeof value !== 'string' ||
    !new RegExp(`^${prefix}\\.[a-z0-9-]+\\.[a-z0-9_]+$`).test(value)
  ) {
    fail(`${context} has invalid message ID ${JSON.stringify(value)}`);
  }
}

export function validateLumaContract(entries) {
  const groups = {};
  const ids = new Set();
  for (const [phrase, phraseIds] of entries) {
    assertIdentifier(phrase, 'Luma phrase');
    if (!Array.isArray(phraseIds) || phraseIds.length === 0) {
      fail(`Luma phrase ${phrase} must contain at least one message ID`);
    }
    groups[phrase] = phraseIds.map((id) => {
      assertMessageId(id, 'luma', `Luma phrase ${phrase}`);
      if (ids.has(id)) {
        fail(`duplicate Luma message ID ${id}`);
      }
      ids.add(id);
      return id;
    });
  }
  if (Object.keys(groups).length === 0) {
    fail('Luma contract must contain at least one phrase');
  }
  return { groups, ids: Array.from(ids).sort() };
}

function nonEmptyString(value, context) {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`${context} must be a non-empty string`);
  }
  return value;
}

export function validateNvapiContract(settings) {
  const sourceCatalog = {};
  const messages = {};
  const settingKeys = new Set();

  const addMessage = (key, value) => {
    sourceCatalog[key] = nonEmptyString(value, key);
    messages[key] = messagePlaceholders(sourceCatalog[key], key);
  };

  for (const setting of settings) {
    if (!setting || typeof setting !== 'object') {
      fail('NVAPI setting must be an object');
    }
    const settingKey = nonEmptyString(setting.key, 'NVAPI setting key');
    if (!/^[a-z0-9_]+$/.test(settingKey) || settingKeys.has(settingKey)) {
      fail(`invalid or duplicate NVAPI setting key ${settingKey}`);
    }
    settingKeys.add(settingKey);
    const prefix = `nvapi.${settingKey}`;
    addMessage(`${prefix}.label`, setting.label);
    if (setting.description !== undefined) {
      addMessage(`${prefix}.description`, setting.description);
    }
    if (setting.values !== undefined && !Array.isArray(setting.values)) {
      fail(`${settingKey}.values must be an array`);
    }

    const wires = new Set();
    for (const option of setting.values ?? []) {
      if (!option || typeof option !== 'object') {
        fail(`${settingKey}.values must contain objects`);
      }
      const wire = nonEmptyString(option.wire, `${settingKey}.value.wire`);
      if (!/^[a-z0-9_]+$/.test(wire) || wires.has(wire)) {
        fail(`invalid or duplicate wire value ${settingKey}.${wire}`);
      }
      wires.add(wire);
      addMessage(`${prefix}.value.${wire}`, option.label);
    }
  }

  return {
    sourceCatalog: sortRecord(sourceCatalog),
    messages: sortRecord(messages),
  };
}
