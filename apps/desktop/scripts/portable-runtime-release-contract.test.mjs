import assert from 'node:assert/strict';
import test from 'node:test';

import { parsePortableRuntimeReleaseContract } from './portable-runtime-release-contract.mjs';

const VALID = `{
  "contractVersion": 1,
  "supervisorCapability": 3,
  "appSessionProtocol": "renderpilot-portable-app-session-v2",
  "minimumPortableSchema": 4,
  "currentSchema": 21
}`;

test('portable runtime release contract accepts exact JSON integer fields', () => {
  const contract = parsePortableRuntimeReleaseContract(VALID);

  assert.equal(contract.contractVersion, 1);
  assert.equal(contract.supervisorCapability, 3);
  assert.equal(contract.appSessionProtocol, 'renderpilot-portable-app-session-v2');
  assert.equal(contract.currentSchema, 21);
});

test('portable runtime release contract rejects non-lexical, overflow, and wire-shape variants', () => {
  const invalid = [
    VALID.replace('"contractVersion": 1', '"contractVersion": 1.0'),
    VALID.replace('"supervisorCapability": 3', '"supervisorCapability": 3e0'),
    VALID.replace('"supervisorCapability": 3', '"supervisorCapability": 2'),
    VALID.replace(
      '"appSessionProtocol": "renderpilot-portable-app-session-v2"',
      '"appSessionProtocol": "renderpilot-portable-app-session-v1"',
    ),
    VALID.replace('"currentSchema": 21', '"currentSchema": 22'),
    VALID.replace('"minimumPortableSchema": 4', '"minimumPortableSchema": "4"'),
    VALID.replace('"minimumPortableSchema": 4,', ''),
    VALID.replace('"currentSchema": 21', '"currentSchema": 21, "unknown": 1'),
    VALID.replace('"currentSchema": 21', '"currentSchema": 20, "currentSchema": 21'),
    VALID.replace('"currentSchema": 21', '"\\u0063urrentSchema": 20, "currentSchema": 21'),
    VALID.replace('"currentSchema": 21', '"CurrentSchema": 21'),
  ];

  for (const source of invalid) {
    assert.throws(() => parsePortableRuntimeReleaseContract(source));
  }
});
