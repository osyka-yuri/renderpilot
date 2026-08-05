import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import { parseExternalSourceCheckArguments } from './external-source-check/arguments.mjs';
import {
  projectLumaManifest,
  verifyLumaSourceContract,
  verifyNvapiSourceContract,
} from './external-source-contracts.mjs';

const lumaContract = {
  schemaVersion: 1,
  phrases: [
    {
      key: 'warning',
      sourceText: 'Careful.',
      messages: [{ id: 'luma.game.warning', context: 'guidance.warning' }],
    },
  ],
};
const lumaManifest = {
  games: [
    {
      id: 'game',
      guidance: [{ id: 'luma.game.warning', kind: 'warning', fallback_text: 'Careful.' }],
    },
  ],
};

test('source-check CLI requires exactly one non-empty producer root', () => {
  assert.deepEqual(parseExternalSourceCheckArguments(['--producer-root', './producer']), {
    producerRoot: path.resolve('./producer'),
  });
  for (const args of [
    [],
    ['--producer-root'],
    ['--producer-root', ''],
    ['--root', './producer'],
    ['--producer-root', './producer', '--extra', 'value'],
  ]) {
    assert.throws(() => parseExternalSourceCheckArguments(args), /Usage:/);
  }
});

test('Luma source check detects missing, stale, changed, and duplicate messages', () => {
  assert.deepEqual(verifyLumaSourceContract(lumaContract, lumaManifest), { messageCount: 1 });
  assert.throws(
    () => verifyLumaSourceContract({ ...lumaContract, phrases: [] }, lumaManifest),
    /at least one phrase/,
  );
  assert.throws(() => verifyLumaSourceContract(lumaContract, { games: [] }), /stale message/);
  assert.throws(
    () =>
      verifyLumaSourceContract(lumaContract, {
        games: [
          {
            ...lumaManifest.games[0],
            guidance: [{ id: 'luma.game.warning', kind: 'warning', fallback_text: 'Changed.' }],
          },
        ],
      }),
    /source text changed/,
  );
  assert.throws(
    () =>
      verifyLumaSourceContract(lumaContract, {
        games: [
          {
            ...lumaManifest.games[0],
            guidance: [
              { id: 'luma.game.warning', kind: 'compatibility', fallback_text: 'Careful.' },
            ],
          },
        ],
      }),
    /context changed/,
  );
  assert.throws(
    () =>
      projectLumaManifest({
        games: [...lumaManifest.games, { id: 'other', guidance: lumaManifest.games[0].guidance }],
      }),
    /duplicate Luma message ID/,
  );
});

test('Luma projection includes availability messages with their blocked context', () => {
  assert.deepEqual(
    projectLumaManifest({
      games: [
        {
          id: 'blocked-game',
          availability: {
            kind: 'blocked',
            message: {
              id: 'luma.blocked-game.availability',
              fallback_text: 'Blocked by policy.',
            },
          },
        },
      ],
    }),
    {
      'luma.blocked-game.availability': {
        context: 'availability.blocked',
        sourceText: 'Blocked by policy.',
      },
    },
  );
  assert.throws(
    () =>
      projectLumaManifest({
        games: [
          {
            id: 'available-game',
            availability: {
              kind: 'available',
              message: {
                id: 'luma.available-game.availability',
                fallback_text: 'Unexpected message.',
              },
            },
          },
        ],
      }),
    /invalid Luma context availability\.available/,
  );
});

test('source checks use the same strict Luma shape and placeholder rules as generation', () => {
  assert.throws(
    () =>
      verifyLumaSourceContract(
        {
          ...lumaContract,
          phrases: [{ ...lumaContract.phrases[0], unexpected: true }],
        },
        lumaManifest,
      ),
    /unknown field "unexpected"/,
  );
  assert.throws(
    () =>
      verifyLumaSourceContract(
        {
          ...lumaContract,
          phrases: [{ ...lumaContract.phrases[0], sourceText: 'Careful {name}.' }],
        },
        lumaManifest,
      ),
    /must not contain placeholders/,
  );
});

const setting = {
  key: 'dlss_sr_mode',
  family: 'sr',
  label: 'Mode',
  description: 'Select a mode.',
  values: [{ wire: 'on', label: 'On' }],
};

test('NVAPI source check compares supported families and ignores producer-only NR', () => {
  const bundled = { schema_version: 1, settings: [setting] };
  const producer = {
    schema_version: 1,
    settings: [setting, { key: 'neural_rendering', family: 'nr', label: 'Neural Rendering' }],
  };
  assert.deepEqual(verifyNvapiSourceContract(bundled, producer), {
    settingCount: 1,
    messageCount: 3,
  });
  assert.throws(
    () =>
      verifyNvapiSourceContract(bundled, {
        schema_version: 1,
        settings: [{ ...setting, description: 'Changed.' }],
      }),
    /source text changed/,
  );
  assert.throws(
    () => verifyNvapiSourceContract(bundled, { schema_version: 1, settings: [] }),
    /stale message/,
  );
  assert.throws(
    () =>
      verifyNvapiSourceContract(bundled, {
        schema_version: 1,
        settings: [setting, { ...setting }],
      }),
    /duplicate NVAPI setting key/,
  );
  assert.throws(
    () =>
      verifyNvapiSourceContract(bundled, {
        schema_version: 1,
        settings: [{ ...setting, label: 'Mode {name}' }],
      }),
    /must not contain placeholders/,
  );
});
