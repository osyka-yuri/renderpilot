import assert from 'node:assert/strict';
import test from 'node:test';

import { parsePortableRuntimeReleaseContract } from './portable-runtime-release-contract.mjs';

const VALID = `{
  "contractVersion": 1,
  "supervisorCapability": 2,
  "appSessionProtocol": "renderpilot-portable-app-session-v1",
  "minimumPortableSchema": 4,
  "currentSchema": 2147483647
}`;

test('portable runtime release contract accepts exact JSON integer fields', () => {
  const contract = parsePortableRuntimeReleaseContract(VALID);

  assert.equal(contract.contractVersion, 1);
  assert.equal(contract.currentSchema, 2147483647);
});

test('portable runtime release contract rejects non-lexical, overflow, and wire-shape variants', () => {
  const invalid = [
    VALID.replace('"contractVersion": 1', '"contractVersion": 1.0'),
    VALID.replace('"supervisorCapability": 2', '"supervisorCapability": 2e0'),
    VALID.replace('"currentSchema": 2147483647', '"currentSchema": 2147483648'),
    VALID.replace('"minimumPortableSchema": 4', '"minimumPortableSchema": "4"'),
    VALID.replace('"minimumPortableSchema": 4,', ''),
    VALID.replace('"currentSchema": 2147483647', '"currentSchema": 16, "unknown": 1'),
    VALID.replace(
      '"currentSchema": 2147483647',
      '"currentSchema": 16, "currentSchema": 2147483647',
    ),
    VALID.replace(
      '"currentSchema": 2147483647',
      '"\\u0063urrentSchema": 16, "currentSchema": 2147483647',
    ),
    VALID.replace('"currentSchema": 2147483647', '"CurrentSchema": 2147483647'),
  ];

  for (const source of invalid) {
    assert.throws(() => parsePortableRuntimeReleaseContract(source));
  }
});
