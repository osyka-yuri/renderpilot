import { analyzeMessageTemplate } from '../ui/src/shared/i18n/messages/template.ts';

const LUMA_CONTEXT_PATTERN = /^(?:guidance\.[a-z][a-z0-9_]*|availability\.blocked)$/;
const LUMA_MESSAGE_ID_PATTERN = /^luma\.[a-z0-9-]+\.[a-z0-9_-]+$/;
const LUMA_PHRASE_KEY_PATTERN = /^[a-z][A-Za-z0-9]*$/;
const NVAPI_IDENTIFIER_PATTERN = /^[a-z0-9_]+$/;

const SUPPORTED_NVAPI_FAMILIES = new Set(['sr', 'fg', 'rr']);

export class ExternalContractValidationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ExternalContractValidationError';
  }
}

function fail(message) {
  throw new ExternalContractValidationError(message);
}

function assertRecord(value, context) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    fail(`${context} must be an object`);
  }
}

function assertExactKeys(value, keys, context) {
  assertRecord(value, context);
  const unknownKeys = Object.keys(value).filter((key) => !keys.includes(key));
  if (unknownKeys.length > 0) {
    fail(`${context} contains unknown field ${JSON.stringify(unknownKeys[0])}`);
  }
  const missingKeys = keys.filter((key) => !Object.hasOwn(value, key));
  if (missingKeys.length > 0) {
    fail(`${context} is missing field ${JSON.stringify(missingKeys[0])}`);
  }
}

function nonEmptyString(value, context) {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`${context} must be a non-empty string`);
  }
  return value;
}

function assertExternalSourceText(value, context) {
  const source = nonEmptyString(value, context);
  const template = analyzeMessageTemplate(source);
  if (!template.valid) {
    fail(`${context} contains invalid placeholder syntax`);
  }
  if (template.placeholders.length > 0) {
    fail(`${context} must not contain placeholders`);
  }
  return source;
}

function sortRecord(record) {
  return Object.fromEntries(
    Object.entries(record).sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0)),
  );
}

/** Validates the checked-in, phrase-deduplicated Luma translation contract. */
export function validateLumaContract(value) {
  assertExactKeys(value, ['schemaVersion', 'phrases'], 'Luma contract');
  if (value.schemaVersion !== 1) {
    fail(`Luma contract has unsupported schemaVersion ${JSON.stringify(value.schemaVersion)}`);
  }
  if (!Array.isArray(value.phrases)) {
    fail('Luma contract phrases must be an array');
  }

  const groups = {};
  const sourceCatalog = {};
  const contexts = {};
  const sourceTexts = new Set();

  for (const phrase of value.phrases) {
    assertExactKeys(phrase, ['key', 'sourceText', 'messages'], 'Luma phrase');
    const key = nonEmptyString(phrase.key, 'Luma phrase key');
    if (!LUMA_PHRASE_KEY_PATTERN.test(key)) {
      fail(`Luma phrase has invalid identifier ${JSON.stringify(key)}`);
    }
    if (Object.hasOwn(groups, key)) {
      fail(`duplicate Luma phrase key ${key}`);
    }

    const sourceText = assertExternalSourceText(phrase.sourceText, `Luma phrase ${key} sourceText`);
    if (sourceTexts.has(sourceText)) {
      fail(`duplicate Luma source text ${JSON.stringify(sourceText)}`);
    }
    sourceTexts.add(sourceText);

    if (!Array.isArray(phrase.messages) || phrase.messages.length === 0) {
      fail(`Luma phrase ${key} must contain at least one message`);
    }

    groups[key] = phrase.messages.map((message) => {
      assertExactKeys(message, ['id', 'context'], `Luma phrase ${key} message`);
      const id = nonEmptyString(message.id, `Luma phrase ${key} message ID`);
      if (!LUMA_MESSAGE_ID_PATTERN.test(id)) {
        fail(`Luma phrase ${key} has invalid message ID ${JSON.stringify(id)}`);
      }
      if (Object.hasOwn(sourceCatalog, id)) {
        fail(`duplicate Luma message ID ${id}`);
      }
      const context = nonEmptyString(message.context, `Luma message ${id} context`);
      if (!LUMA_CONTEXT_PATTERN.test(context)) {
        fail(`Luma message ${id} has invalid context ${JSON.stringify(context)}`);
      }
      sourceCatalog[id] = sourceText;
      contexts[id] = context;
      return id;
    });
  }

  if (Object.keys(groups).length === 0) {
    fail('Luma contract must contain at least one phrase');
  }
  return {
    groups,
    sourceCatalog: sortRecord(sourceCatalog),
    contexts: sortRecord(contexts),
  };
}

/** Extracts every producer-owned Luma message that may be shown to a user. */
export function projectLumaManifest(manifest) {
  assertRecord(manifest, 'Luma manifest');
  if (!Array.isArray(manifest.games)) {
    fail('Luma manifest must contain a games array');
  }

  const projection = {};
  const addMessage = (message, context) => {
    assertRecord(message, context);
    if (!LUMA_CONTEXT_PATTERN.test(context)) {
      fail(`invalid Luma context ${context}`);
    }
    const id = nonEmptyString(message.id, `${context}.id`);
    if (!LUMA_MESSAGE_ID_PATTERN.test(id)) {
      fail(`${context} has invalid message ID ${JSON.stringify(id)}`);
    }
    if (Object.hasOwn(projection, id)) {
      fail(`duplicate Luma message ID ${id}`);
    }
    projection[id] = {
      context,
      sourceText: assertExternalSourceText(message.fallback_text, `${context}.fallback_text`),
    };
  };

  for (const game of manifest.games) {
    assertRecord(game, 'Luma game');
    const gameId = nonEmptyString(game.id, 'Luma game.id');
    if (game.guidance !== undefined && !Array.isArray(game.guidance)) {
      fail(`Luma game ${gameId} guidance must be an array`);
    }
    for (const message of game.guidance ?? []) {
      const kind = nonEmptyString(message?.kind, `Luma game ${gameId} guidance.kind`);
      addMessage(message, `guidance.${kind}`);
    }
    if (game.availability?.message !== undefined) {
      const kind = nonEmptyString(game.availability.kind, `Luma game ${gameId} availability.kind`);
      addMessage(game.availability.message, `availability.${kind}`);
    }
  }

  return sortRecord(projection);
}

/** Projects a bundled/producer NVAPI catalog to the locale-aware UI contract. */
export function projectSupportedNvapiCatalog(value) {
  assertRecord(value, 'NVAPI catalog');
  if (value.schema_version !== 1) {
    fail(`NVAPI catalog has unsupported schema_version ${JSON.stringify(value.schema_version)}`);
  }
  if (!Array.isArray(value.settings)) {
    fail('NVAPI catalog must contain a settings array');
  }

  const settings = value.settings.filter((setting) =>
    SUPPORTED_NVAPI_FAMILIES.has(setting?.family),
  );
  const sourceCatalog = {};
  const settingKeys = new Set();

  const addMessage = (key, source) => {
    if (Object.hasOwn(sourceCatalog, key)) {
      fail(`duplicate NVAPI message key ${key}`);
    }
    sourceCatalog[key] = assertExternalSourceText(source, key);
  };

  for (const setting of settings) {
    assertRecord(setting, 'NVAPI setting');
    const key = nonEmptyString(setting.key, 'NVAPI setting key');
    if (!NVAPI_IDENTIFIER_PATTERN.test(key) || settingKeys.has(key)) {
      fail(`invalid or duplicate NVAPI setting key ${key}`);
    }
    settingKeys.add(key);

    const prefix = `nvapi.${key}`;
    addMessage(`${prefix}.label`, setting.label);
    if (setting.description !== undefined) {
      addMessage(`${prefix}.description`, setting.description);
    }
    if (setting.values !== undefined && !Array.isArray(setting.values)) {
      fail(`${key}.values must be an array`);
    }

    const wires = new Set();
    for (const option of setting.values ?? []) {
      assertRecord(option, `${key}.values entry`);
      const wire = nonEmptyString(option.wire, `${key}.value.wire`);
      if (!NVAPI_IDENTIFIER_PATTERN.test(wire) || wires.has(wire)) {
        fail(`invalid or duplicate wire value ${key}.${wire}`);
      }
      wires.add(wire);
      addMessage(`${prefix}.value.${wire}`, option.label);
    }
  }

  return {
    settingCount: settings.length,
    sourceCatalog: sortRecord(sourceCatalog),
  };
}
