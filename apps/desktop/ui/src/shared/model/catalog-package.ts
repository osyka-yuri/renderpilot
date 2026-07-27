export type ReleaseChannel = 'stable' | 'beta' | 'preview' | 'debug';

export type CatalogRelease = Readonly<{
  version: string;
  channel: ReleaseChannel;
  label: string | null;
}>;

export type CatalogPackageAvailability = 'available' | 'local_only';

export type CatalogCandidatePackage = Readonly<{
  package_id: string;
  release: CatalogRelease;
  availability: CatalogPackageAvailability;
  automatic_selection_allowed: boolean;
}>;
