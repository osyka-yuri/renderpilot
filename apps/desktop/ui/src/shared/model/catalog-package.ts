export type ReleaseChannel = 'stable' | 'beta' | 'preview' | 'debug';

export type CatalogRelease = Readonly<{
  version: string;
  channel: ReleaseChannel;
  label: string | null;
  components?: Readonly<Record<string, string>>;
}>;

export type CatalogPackageAvailability = 'available' | 'local_only';

export type CatalogSourceBuildToolchain = Readonly<{
  runner_image: string;
  compiler: string;
  linker: string;
  windows_sdk: string;
  cmake: string;
}>;

export type CatalogSourceReceipt = Readonly<{
  repository: string;
  version: string;
  tag: string | null;
  tag_object_sha: string | null;
  commit_sha: string | null;
  archive_url: string;
  archive_sha256: string;
}>;

export type CatalogSourcePatchReceipt = Readonly<{
  source: string;
  target: string;
  descriptor_sha256: string;
  original_sha256: string;
  patched_sha256: string;
}>;

export type CatalogSourceBuildProvenance = Readonly<{
  kind: 'source_build';
  sources: Readonly<Record<string, CatalogSourceReceipt>>;
  build_revision: number;
  recipe_sha256: string;
  verification_policy_sha256: string;
  patches: Readonly<Record<string, CatalogSourcePatchReceipt>>;
  toolchain: CatalogSourceBuildToolchain;
}>;

export type CatalogNugetProvenance = Readonly<{
  kind: 'nuget';
  package_id: string;
  version: string;
  package_sha512: string;
}>;

export type CatalogGithubReleaseProvenance = Readonly<{
  kind: 'github_release';
  repository: string;
  tag: string;
  commit_sha: string;
}>;

export type CatalogCandidateProvenance =
  CatalogNugetProvenance | CatalogGithubReleaseProvenance | CatalogSourceBuildProvenance;

export type CatalogLegalDocument = Readonly<{
  legal_document_id: string;
  kind: 'license' | 'notice';
  title: string;
  format: 'text' | 'pdf';
  file_name: string;
  content_url: string;
}>;

export type CatalogCandidatePresentation = Readonly<{
  variant: string;
  architecture: 'X86' | 'X64';
  unsigned: boolean;
  provenance: CatalogCandidateProvenance | null;
  legal_documents: readonly CatalogLegalDocument[];
}>;

export type CatalogCandidatePackage = Readonly<{
  package_id: string;
  release: CatalogRelease;
  availability: CatalogPackageAvailability;
  automatic_selection_allowed: boolean;
  presentation?: CatalogCandidatePresentation | null;
}>;
