import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { parseExternalSourceCheckArguments } from './external-source-check/arguments.mjs';
import {
  verifyLumaSourceContract,
  verifyNvapiSourceContract,
} from './external-source-contracts.mjs';

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPOSITORY_ROOT = path.resolve(APP_ROOT, '..', '..');

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'));
}

const { producerRoot } = parseExternalSourceCheckArguments(process.argv.slice(2));
const [lumaContract, lumaManifest, bundledNvapi, producerNvapi] = await Promise.all([
  readJson(path.join(APP_ROOT, 'ui/src/shared/i18n/messages/overrides/luma/contract.json')),
  readJson(path.join(producerRoot, 'addons/v1/luma.json')),
  readJson(
    path.join(
      REPOSITORY_ROOT,
      'crates/renderpilot-orchestration/src/dlss/bundled/dlss_settings.json',
    ),
  ),
  readJson(path.join(producerRoot, 'dlss_settings.json')),
]);

const luma = verifyLumaSourceContract(lumaContract, lumaManifest);
const nvapi = verifyNvapiSourceContract(bundledNvapi, producerNvapi);
console.log(
  `Verified external i18n sources: Luma ${luma.messageCount} messages; NVAPI ${nvapi.settingCount} settings / ${nvapi.messageCount} messages.`,
);
