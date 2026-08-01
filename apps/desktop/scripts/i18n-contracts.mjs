import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createContractVersion, createSemanticContract } from './i18n-contracts/contract.mjs';
import { formatGeneratedSource as formatSource } from './i18n-contracts/formatter.mjs';
import {
  parseEnglishSource,
  parseJsonSource,
  parseLumaSource,
  parseNvapiSource,
  parsePluralCategorySource,
} from './i18n-contracts/parser.mjs';
import {
  analyzeMessageTemplate,
  validateEnglishContract,
  validateLumaContract,
  validateNvapiContract,
  validatePluralCategories,
} from './i18n-contracts/validator.mjs';
import {
  renderContractVersion,
  renderLumaContract,
  renderNvapiContract,
} from './i18n-contracts/renderer.mjs';

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPOSITORY_ROOT = path.resolve(APP_ROOT, '..', '..');
const FORMAT_CONFIG = path.join(APP_ROOT, '.oxfmtrc.json');

const INPUTS = {
  english: path.join(APP_ROOT, 'ui/src/shared/i18n/messages/en.ts'),
  messageModel: path.join(APP_ROOT, 'ui/src/shared/i18n/messages/model.ts'),
  luma: path.join(APP_ROOT, 'ui/src/shared/i18n/messages/overrides/luma/contract.json'),
  nvapi: path.join(
    REPOSITORY_ROOT,
    'crates/renderpilot-orchestration/src/dlss/bundled/dlss_settings.json',
  ),
};

const OUTPUTS = {
  contractVersion: path.join(APP_ROOT, 'ui/src/shared/i18n/messages/generated/contract-version.ts'),
  luma: path.join(APP_ROOT, 'ui/src/shared/i18n/messages/overrides/luma/schema.ts'),
  nvapi: path.join(APP_ROOT, 'ui/src/shared/i18n/messages/overrides/nvapi/contract.generated.ts'),
};

function withInputContext(filePath, operation) {
  try {
    return operation();
  } catch (cause) {
    const prefix = 'i18n contract generation failed: ';
    const detail =
      cause instanceof Error && cause.message.startsWith(prefix)
        ? cause.message.slice(prefix.length)
        : cause instanceof Error
          ? cause.message
          : String(cause);
    const relativePath = path.relative(APP_ROOT, filePath).replaceAll(path.sep, '/');
    throw new Error(`${prefix}${relativePath}: ${detail}`, { cause });
  }
}

export function parseEnglishContract(sourceText) {
  return withInputContext(INPUTS.english, () =>
    validateEnglishContract(parseEnglishSource(sourceText, INPUTS.english)),
  );
}

export function parsePluralCategories(sourceText) {
  return withInputContext(INPUTS.messageModel, () =>
    validatePluralCategories(parsePluralCategorySource(sourceText, INPUTS.messageModel)),
  );
}

export function parseLumaContract(value) {
  return withInputContext(INPUTS.luma, () => validateLumaContract(parseLumaSource(value)));
}

export function parseNvapiContract(value) {
  return parseValidatedNvapiContract(value).sourceCatalog;
}

function parseValidatedNvapiContract(value) {
  return withInputContext(INPUTS.nvapi, () => validateNvapiContract(parseNvapiSource(value)));
}

export { analyzeMessageTemplate };

export function formatGeneratedSource(filePath, source, configPath = FORMAT_CONFIG) {
  return formatSource(filePath, source, configPath, APP_ROOT);
}

export async function createI18nContractOutputs() {
  const [englishText, messageModelText, lumaText, nvapiText] = await Promise.all([
    readFile(INPUTS.english, 'utf8'),
    readFile(INPUTS.messageModel, 'utf8'),
    readFile(INPUTS.luma, 'utf8'),
    readFile(INPUTS.nvapi, 'utf8'),
  ]);

  const english = parseEnglishContract(englishText);
  const pluralCategories = parsePluralCategories(messageModelText);
  const luma = parseLumaContract(withInputContext(INPUTS.luma, () => parseJsonSource(lumaText)));
  const nvapi = parseValidatedNvapiContract(
    withInputContext(INPUTS.nvapi, () => parseJsonSource(nvapiText)),
  );
  const contract = createSemanticContract({ english, pluralCategories, luma, nvapi });

  return new Map(
    await Promise.all(
      [
        [OUTPUTS.contractVersion, renderContractVersion(createContractVersion(contract))],
        [OUTPUTS.luma, renderLumaContract(luma)],
        [OUTPUTS.nvapi, renderNvapiContract(nvapi.sourceCatalog)],
      ].map(async ([filePath, source]) => [
        filePath,
        await formatGeneratedSource(filePath, source),
      ]),
    ),
  );
}

export async function checkI18nContractOutputs(outputs) {
  const resolvedOutputs = outputs ?? (await createI18nContractOutputs());
  const stale = [];
  for (const [filePath, expected] of resolvedOutputs) {
    let actual = null;
    try {
      actual = await readFile(filePath, 'utf8');
    } catch (error) {
      if (error?.code !== 'ENOENT') {
        throw error;
      }
    }
    if (actual !== expected) {
      stale.push(path.relative(APP_ROOT, filePath).replaceAll(path.sep, '/'));
    }
  }
  return stale;
}

export async function writeI18nContractOutputs(outputs) {
  const resolvedOutputs = outputs ?? (await createI18nContractOutputs());
  for (const [filePath, source] of resolvedOutputs) {
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, source, 'utf8');
  }
}
