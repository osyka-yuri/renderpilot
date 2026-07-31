import { describe, expect, it } from 'vitest';

import type { OperationMetadata, OperationSummary } from '@entities/operation';
import { t } from '@shared/i18n';

import { createOperationViewModel } from './operations-page-presenter';

function operation(overrides: Partial<OperationSummary> = {}): OperationSummary {
  return {
    operation_id: 'op-1',
    kind: 'replace_component',
    status: 'completed',
    created_at: 1_700_000_000_000,
    completed_at: 1_700_000_010_000,
    item_count: 2,
    component_id: 'component-1',
    metadata: null,
    ...overrides,
  };
}

function metadata(overrides: Partial<OperationMetadata> = {}): OperationMetadata {
  return {
    game_name: 'Test Game',
    technology: 'dlss_super_resolution',
    from_version: null,
    to_version: 'unknown',
    ...overrides,
  };
}

function present(operation: OperationSummary) {
  return createOperationViewModel(operation, 'en');
}

describe('createOperationViewModel', () => {
  it('passes the id and item count through', () => {
    const vm = present(operation({ operation_id: 'op-9', item_count: 5 }));

    expect(vm.id).toBe('op-9');
    expect(vm.itemCount).toBe(5);
  });

  it('produces non-empty kind / status labels', () => {
    const vm = present(operation());

    expect(vm.kindLabel.length).toBeGreaterThan(0);
    expect(vm.statusLabel.length).toBeGreaterThan(0);
  });

  it('uses metadata.technology for the library type when present', () => {
    const vm = present(operation({ metadata: metadata({ technology: 'dlss_super_resolution' }) }));

    // metadata.technology is routed through formatLabel (canonical technology label).
    expect(vm.libraryType).toBe('DLSS Super Resolution');
  });

  it('falls back to the component-id label map when metadata.technology is missing', () => {
    expect(
      present(operation({ metadata: null, component_id: 'component:DLSS Super Resolution' }))
        .libraryType,
    ).toBe('DLSS Super Resolution');

    expect(
      present(operation({ metadata: null, component_id: 'game:streamline:sl' })).libraryType,
    ).toBe('NVIDIA Streamline');

    expect(present(operation({ metadata: null, component_id: 'comp:fsr:dx12' })).libraryType).toBe(
      'AMD FSR',
    );
  });

  it('returns "-" for library when neither metadata nor component_id is available', () => {
    expect(present(operation({ metadata: null, component_id: undefined })).libraryType).toBe('-');
  });

  it('resolves the game name from metadata, otherwise a dash', () => {
    expect(present(operation({ metadata: metadata({ game_name: 'Elden Ring' }) })).gameName).toBe(
      'Elden Ring',
    );

    expect(present(operation({ metadata: null })).gameName).toBe('-');
  });

  it('exposes from/to versions from metadata', () => {
    const vm = present(
      operation({ metadata: metadata({ from_version: '3.5', to_version: '3.7' }) }),
    );
    expect(vm.fromVersion).toBe('3.5');
    expect(vm.toVersion).toBe('3.7');

    const none = present(operation({ metadata: null }));
    expect(none.fromVersion).toBeNull();
    expect(none.toVersion).toBeNull();
  });

  it('has no completed duration when completed_at is null', () => {
    expect(present(operation({ completed_at: null })).completedDurationText).toBeNull();
  });

  it('uses localized unknown copy when the timestamp is invalid', () => {
    const vm = present(operation({ created_at: Number.NaN }));

    expect(vm.createdAtText).toBe(t('common.unknown'));
  });
});
