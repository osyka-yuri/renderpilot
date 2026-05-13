import {
  createPresentedLibraries,
  formatCompactLibraryLabel,
  libraryVendorOrder,
  type LibraryVendorKey as LibraryFilterVendorKey,
  vendorLabelForLibraryVendorKey,
} from '@shared/graphics';

export type LibraryFilterOption = {
  value: string;
  label: string;
  vendorKey: LibraryFilterVendorKey;
  vendorLabel: string;
};

export type GroupedLibraryFilterOptions = {
  vendorKey: LibraryFilterVendorKey;
  vendorLabel: string;
  options: LibraryFilterOption[];
};

export function buildLibraryFilterOptions(values: readonly string[]): LibraryFilterOption[] {
  return createPresentedLibraries(values, formatCompactLibraryLabel).map((library) => ({
    value: library.tag,
    label: library.label,
    vendorKey: library.vendorKey,
    vendorLabel: vendorLabelForLibraryVendorKey(library.vendorKey),
  }));
}

export function groupLibraryFilterOptions(
  options: readonly LibraryFilterOption[],
): GroupedLibraryFilterOptions[] {
  const groupsByVendor = new Map<LibraryFilterVendorKey, GroupedLibraryFilterOptions>(
    libraryVendorOrder.map((vendorKey) => [
      vendorKey,
      {
        vendorKey,
        vendorLabel: vendorLabelForLibraryVendorKey(vendorKey),
        options: [],
      },
    ]),
  );

  for (const option of options) {
    const group = groupsByVendor.get(option.vendorKey);

    if (group) {
      group.options.push(option);
    }
  }

  return libraryVendorOrder.flatMap((vendorKey) => {
    const group = groupsByVendor.get(vendorKey);

    if (!group || group.options.length === 0) {
      return [];
    }

    return [group];
  });
}

export function mergeVendorDraftLibraries(
  currentLibraries: readonly string[],
  vendorOptions: readonly Pick<LibraryFilterOption, 'value'>[],
  nextVendorLibraries: readonly string[],
): string[] {
  const vendorValues = new Set(vendorOptions.map((option) => option.value));

  return Array.from(
    new Set([
      ...currentLibraries.filter((value) => !vendorValues.has(value)),
      ...nextVendorLibraries,
    ]),
  );
}

export function selectedLibrariesForVendor(
  currentLibraries: readonly string[],
  vendorOptions: readonly Pick<LibraryFilterOption, 'value'>[],
): string[] {
  const vendorValues = new Set(vendorOptions.map((option) => option.value));

  return currentLibraries.filter((value) => vendorValues.has(value));
}
