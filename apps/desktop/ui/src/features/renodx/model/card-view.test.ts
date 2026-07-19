import { describe, expect, it } from 'vitest';

import { getCardView, type CardViewSource, type RenoDxCardView } from './card-view';

const BASE: CardViewSource = {
  loading: false,
  loaded: true,
  loadError: null,
  isInstalled: false,
  isBlockedByOtherAddon: false,
  isExternal: false,
  isNativeHdr: false,
  isBlacklisted: false,
  isUnsupported: false,
  isIncompatible: false,
  isInstallable: false,
};

type CardViewCase = {
  name: string;
  flags: Partial<CardViewSource>;
  expected: RenoDxCardView;
};

const cardViewCases = [
  {
    name: 'loading before the first load completes',
    flags: { loading: true, loaded: false },
    expected: 'loading',
  },
  { name: 'load error', flags: { loadError: 'boom' }, expected: 'load-error' },
  { name: 'installed', flags: { isInstalled: true }, expected: 'installed' },
  {
    name: 'blocked by other addon',
    flags: { isBlockedByOtherAddon: true },
    expected: 'blocked-by-other-addon',
  },
  { name: 'external', flags: { isExternal: true }, expected: 'external' },
  { name: 'native HDR', flags: { isNativeHdr: true }, expected: 'native-hdr' },
  { name: 'blacklisted', flags: { isBlacklisted: true }, expected: 'blacklisted' },
  { name: 'unsupported', flags: { isUnsupported: true }, expected: 'unsupported' },
  { name: 'incompatible', flags: { isIncompatible: true }, expected: 'incompatible' },
  { name: 'installable', flags: { isInstallable: true }, expected: 'installable' },
  { name: 'unavailable fallback when nothing matches', flags: {}, expected: 'unavailable' },
  // A retained failure remains visible while its explicit retry is in progress.
  {
    name: 'load error wins over a retry loading flag and installed',
    flags: { loading: true, loaded: false, loadError: 'boom', isInstalled: true },
    expected: 'load-error',
  },
  // Priority: once loaded, a stale `loading=true` (e.g. a background refresh)
  // no longer forces the loading view.
  {
    name: 'loaded overrides a stale loading flag',
    flags: { loading: true, loaded: true, isInstalled: true },
    expected: 'installed',
  },
  // Priority: a load error wins over every outcome-derived flag.
  {
    name: 'load error wins over installed',
    flags: { loadError: 'boom', isInstalled: true },
    expected: 'load-error',
  },
  // Priority order among the mutually-exclusive outcome flags themselves.
  {
    name: 'installed wins over external',
    flags: { isInstalled: true, isExternal: true },
    expected: 'installed',
  },
  {
    name: 'installed wins over blocked by other addon',
    flags: { isInstalled: true, isBlockedByOtherAddon: true },
    expected: 'installed',
  },
  {
    name: 'blocked by other addon wins over external',
    flags: { isBlockedByOtherAddon: true, isExternal: true },
    expected: 'blocked-by-other-addon',
  },
  {
    name: 'external wins over native HDR',
    flags: { isExternal: true, isNativeHdr: true },
    expected: 'external',
  },
  {
    name: 'native HDR wins over blacklisted',
    flags: { isNativeHdr: true, isBlacklisted: true },
    expected: 'native-hdr',
  },
  {
    name: 'blacklisted wins over unsupported',
    flags: { isBlacklisted: true, isUnsupported: true },
    expected: 'blacklisted',
  },
  {
    name: 'unsupported wins over incompatible',
    flags: { isUnsupported: true, isIncompatible: true },
    expected: 'unsupported',
  },
  {
    name: 'incompatible wins over installable',
    flags: { isIncompatible: true, isInstallable: true },
    expected: 'incompatible',
  },
] satisfies readonly CardViewCase[];

describe('getCardView', () => {
  it.each(cardViewCases)('$name', ({ flags, expected }) => {
    expect(getCardView({ ...BASE, ...flags })).toBe(expected);
  });
});
