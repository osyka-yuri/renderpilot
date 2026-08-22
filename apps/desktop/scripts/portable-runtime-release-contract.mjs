import { readFileSync } from 'node:fs';
import { getNodeValue, parseTree } from 'jsonc-parser';

import { fail } from './release-manifest-common.mjs';

const CONTRACT_URL = new URL(
  '../../../data/contracts/portable-runtime-release.json',
  import.meta.url,
);
const EXPECTED_FIELDS = [
  'appSessionProtocol',
  'contractVersion',
  'currentSchema',
  'minimumPortableSchema',
  'supervisorCapability',
];
const JSON_INTEGER = /^(?:0|[1-9][0-9]*)$/;

export function parsePortableRuntimeReleaseContract(source) {
  const errors = [];
  const tree = parseTree(source, errors, {
    allowTrailingComma: false,
    disallowComments: true,
  });
  if (errors.length !== 0 || !tree || tree.type !== 'object' || !Array.isArray(tree.children)) {
    fail('Portable runtime release contract is not valid exact JSON.');
  }

  const fields = new Map();
  for (const property of tree.children) {
    const [nameNode, valueNode] = property.children ?? [];
    if (property.type !== 'property' || nameNode?.type !== 'string' || !valueNode) {
      fail('Portable runtime release contract has an unsupported shape or range.');
    }
    const name = getNodeValue(nameNode);
    if (typeof name !== 'string' || fields.has(name) || !EXPECTED_FIELDS.includes(name)) {
      fail('Portable runtime release contract has an unsupported shape or range.');
    }
    fields.set(name, valueNode);
  }
  if (
    fields.size !== EXPECTED_FIELDS.length ||
    EXPECTED_FIELDS.some((field) => !fields.has(field))
  ) {
    fail('Portable runtime release contract has an unsupported shape or range.');
  }

  const appSessionProtocol = getStringField(fields, 'appSessionProtocol');
  const contractVersion = getIntegerField(source, fields, 'contractVersion');
  const supervisorCapability = getIntegerField(source, fields, 'supervisorCapability');
  const minimumPortableSchema = getIntegerField(source, fields, 'minimumPortableSchema');
  const currentSchema = getIntegerField(source, fields, 'currentSchema');
  const contract = {
    appSessionProtocol,
    contractVersion,
    currentSchema,
    minimumPortableSchema,
    supervisorCapability,
  };
  if (
    contract.contractVersion !== 1 ||
    contract.supervisorCapability !== 3 ||
    contract.appSessionProtocol !== 'renderpilot-portable-app-session-v2' ||
    contract.minimumPortableSchema !== 4 ||
    contract.currentSchema !== 18
  ) {
    fail('Portable runtime release contract has an unsupported shape or range.');
  }
  return Object.freeze(contract);
}

function getStringField(fields, name) {
  const node = fields.get(name);
  const value = getNodeValue(node);
  if (node.type !== 'string' || typeof value !== 'string') {
    fail(`Portable runtime release contract field ${name} must be a JSON string.`);
  }
  return value;
}

function getIntegerField(source, fields, name) {
  const node = fields.get(name);
  const raw = source.slice(node.offset, node.offset + node.length);
  if (node.type !== 'number' || !JSON_INTEGER.test(raw)) {
    fail(`Portable runtime release contract field ${name} must be a JSON integer.`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) {
    fail(`Portable runtime release contract field ${name} is outside the exact integer range.`);
  }
  return value;
}

function loadPortableRuntimeReleaseContract() {
  return parsePortableRuntimeReleaseContract(readFileSync(CONTRACT_URL, 'utf8'));
}

const PORTABLE_RUNTIME_RELEASE_CONTRACT = loadPortableRuntimeReleaseContract();
export const PORTABLE_RUNTIME_RELEASE_CONTRACT_VERSION =
  PORTABLE_RUNTIME_RELEASE_CONTRACT.contractVersion;
export const PORTABLE_SUPERVISOR_CAPABILITY =
  PORTABLE_RUNTIME_RELEASE_CONTRACT.supervisorCapability;
export const PORTABLE_APP_SESSION_PROTOCOL = PORTABLE_RUNTIME_RELEASE_CONTRACT.appSessionProtocol;
export const MINIMUM_PORTABLE_SCHEMA = PORTABLE_RUNTIME_RELEASE_CONTRACT.minimumPortableSchema;
export const CURRENT_PORTABLE_SCHEMA = PORTABLE_RUNTIME_RELEASE_CONTRACT.currentSchema;
