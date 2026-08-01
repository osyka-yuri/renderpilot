import { createHash } from 'node:crypto';

export const CONTRACT_SCHEMA_VERSION = 1;

export function createSemanticContract({ english, pluralCategories, luma, nvapi }) {
  return {
    schemaVersion: CONTRACT_SCHEMA_VERSION,
    english,
    pluralCategories,
    dynamic: {
      luma: Object.fromEntries(luma.ids.map((id) => [id, []])),
      nvapi: nvapi.messages,
    },
  };
}

export function createContractVersion(contract) {
  const hash = createHash('sha256').update(JSON.stringify(contract)).digest('hex');
  return `i18n-v${CONTRACT_SCHEMA_VERSION}:${hash}`;
}
