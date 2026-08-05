import {
  ExternalContractValidationError,
  projectLumaManifest as projectLumaManifestCore,
  projectSupportedNvapiCatalog as projectSupportedNvapiCatalogCore,
  validateLumaContract,
} from './external-contract-core.mjs';

function fail(message, cause) {
  throw new Error(`external i18n source check failed: ${message}`, { cause });
}

function validate(operation) {
  try {
    return operation();
  } catch (cause) {
    if (cause instanceof ExternalContractValidationError) {
      fail(cause.message, cause);
    }
    throw cause;
  }
}

export function projectLumaManifest(manifest) {
  return validate(() => projectLumaManifestCore(manifest));
}

export function verifyLumaSourceContract(contract, manifest) {
  return validate(() => {
    const checked = validateLumaContract(contract);
    const actual = projectLumaManifestCore(manifest);
    const expectedIds = new Set(Object.keys(checked.sourceCatalog));
    const actualIds = new Set(Object.keys(actual));

    for (const id of actualIds.difference(expectedIds)) {
      fail(`Luma contract is missing producer message ${id}`);
    }
    for (const id of expectedIds.difference(actualIds)) {
      fail(`Luma contract contains stale message ${id}`);
    }
    for (const id of expectedIds.intersection(actualIds)) {
      if (checked.sourceCatalog[id] !== actual[id].sourceText) {
        fail(`Luma source text changed for ${id}`);
      }
      if (checked.contexts[id] !== actual[id].context) {
        fail(`Luma context changed for ${id}`);
      }
    }

    return { messageCount: actualIds.size };
  });
}

export function projectSupportedNvapiCatalog(value) {
  return validate(() => projectSupportedNvapiCatalogCore(value));
}

export function verifyNvapiSourceContract(bundled, producer) {
  return validate(() => {
    const expected = projectSupportedNvapiCatalogCore(bundled);
    const actual = projectSupportedNvapiCatalogCore(producer);
    const expectedKeys = new Set(Object.keys(expected.sourceCatalog));
    const actualKeys = new Set(Object.keys(actual.sourceCatalog));

    for (const key of actualKeys.difference(expectedKeys)) {
      fail(`bundled NVAPI catalog is missing producer message ${key}`);
    }
    for (const key of expectedKeys.difference(actualKeys)) {
      fail(`bundled NVAPI catalog contains stale message ${key}`);
    }
    for (const key of expectedKeys.intersection(actualKeys)) {
      if (expected.sourceCatalog[key] !== actual.sourceCatalog[key]) {
        fail(`NVAPI source text changed for ${key}`);
      }
    }

    return { settingCount: actual.settingCount, messageCount: actualKeys.size };
  });
}
