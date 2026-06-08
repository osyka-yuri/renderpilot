import { describe, expect, it } from 'vitest';

import type { OperationSummary } from '@entities/operation';

import { createOperationViewModel } from './operations-page-presenter';

function operation(overrides: Partial<OperationSummary> = {}): OperationSummary {
  return {
    operation_id: 'op-1',
    kind: 'replace_component',
    status: 'completed',
    created_at: 1_700_000_000_000,
    completed_at: 1_700_000_010_000,
    item_count: 2,
    ...overrides,
  };
}

describe('createOperationViewModel', () => {
  it('passes the id and item count through', () => {
    const vm = createOperationViewModel(operation({ operation_id: 'op-9', item_count: 5 }));

    expect(vm.id).toBe('op-9');
    expect(vm.itemCount).toBe(5);
  });

  it('produces non-empty kind / status labels', () => {
    const vm = createOperationViewModel(operation());

    expect(vm.kindLabel.length).toBeGreaterThan(0);
    expect(vm.statusLabel.length).toBeGreaterThan(0);
  });

  it('uses metadata.library for the library type when present', () => {
    const vm = createOperationViewModel(
      operation({ metadata: { library: 'dlss_super_resolution' } }),
    );

    // metadata.library is routed through formatLabel (canonical library label).
    expect(vm.libraryType).toBe('DLSS Super Resolution');
  });

  it('falls back to the component-id label map when metadata.library is missing', () => {
    expect(
      createOperationViewModel(
        operation({ metadata: null, component_id: 'component:DLSS Super Resolution' }),
      ).libraryType,
    ).toBe('DLSS Super Resolution');

    expect(
      createOperationViewModel(operation({ metadata: null, component_id: 'game:streamline:sl' }))
        .libraryType,
    ).toBe('NVIDIA Streamline');

    expect(
      createOperationViewModel(operation({ metadata: null, component_id: 'comp:fsr:dx12' }))
        .libraryType,
    ).toBe('AMD FSR');
  });

  it('returns "-" for library when neither metadata nor component_id is available', () => {
    expect(
      createOperationViewModel(operation({ metadata: null, component_id: undefined })).libraryType,
    ).toBe('-');
  });

  it('resolves the game name from metadata, otherwise a dash', () => {
    expect(
      createOperationViewModel(operation({ metadata: { game_name: 'Elden Ring' } })).gameName,
    ).toBe('Elden Ring');

    expect(createOperationViewModel(operation({ metadata: null })).gameName).toBe('-');
  });

  it('exposes from/to versions from metadata', () => {
    const vm = createOperationViewModel(
      operation({ metadata: { from_version: '3.5', to_version: '3.7' } }),
    );
    expect(vm.fromVersion).toBe('3.5');
    expect(vm.toVersion).toBe('3.7');

    const none = createOperationViewModel(operation({ metadata: null }));
    expect(none.fromVersion).toBeNull();
    expect(none.toVersion).toBeNull();
  });

  it('has no completed duration when completed_at is null', () => {
    expect(
      createOperationViewModel(operation({ completed_at: null })).completedDurationText,
    ).toBeNull();
  });
});
