import assert from 'node:assert/strict';
import test from 'node:test';

import { parsePortableRuntimeReleaseContract } from './portable-runtime-release-contract.mjs';

const VALID = `{
  "contractVersion": 1,
  "supervisorCapability": 3,
  "appSessionProtocol": "renderpilot-portable-app-session-v2",
  "minimumPortableSchema": 4,
  "currentSchema": 17
}`;

test('portable runtime release contract accepts exact JSON integer fields', () => {
  const contract = parsePortableRuntimeReleaseContract(VALID);

  assert.equal(contract.contractVersion, 1);
  assert.equal(contract.supervisorCapability, 3);
  assert.equal(contract.appSessionProtocol, 'renderpilot-portable-app-session-v2');
  assert.equal(contract.currentSchema, 17);
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
    VALID.replace('"currentSchema": 17', '"currentSchema": 18'),
    VALID.replace('"minimumPortableSchema": 4', '"minimumPortableSchema": "4"'),
    VALID.replace('"minimumPortableSchema": 4,', ''),
    VALID.replace('"currentSchema": 17', '"currentSchema": 17, "unknown": 1'),
    VALID.replace('"currentSchema": 17', '"currentSchema": 16, "currentSchema": 17'),
    VALID.replace('"currentSchema": 17', '"\\u0063urrentSchema": 16, "currentSchema": 17'),
    VALID.replace('"currentSchema": 17', '"CurrentSchema": 17'),
  ];

  for (const source of invalid) {
    assert.throws(() => parsePortableRuntimeReleaseContract(source));
  }
});
