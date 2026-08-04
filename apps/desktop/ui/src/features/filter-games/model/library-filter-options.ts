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
  const groupsByVendor = Map.groupBy(options, (option) => option.vendorKey);

  return libraryVendorOrder.flatMap((vendorKey) => {
    const vendorOptions = groupsByVendor.get(vendorKey);

    if (!vendorOptions || vendorOptions.length === 0) {
      return [];
    }

    return [
      {
        vendorKey,
        vendorLabel: vendorLabelForLibraryVendorKey(vendorKey),
        options: vendorOptions,
      },
    ];
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
