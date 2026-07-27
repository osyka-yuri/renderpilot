import type { CatalogPackageAvailability, CatalogRelease } from '@shared/model';

export type RuntimeCompatibility = Readonly<{ kind: 'd3d12_sdk'; version: number }>;

export type LibraryTarget = Readonly<{
  os: string;
  architecture: 'X86' | 'X64';
  compatibility?: RuntimeCompatibility;
}>;

export type Signature = Readonly<
  | {
      status: 'signed';
      subject?: string;
      thumbprint?: string;
      signed_at: string | null;
    }
  | { status: 'unsigned' }
>;

/** Canonical package release used for presentation, ordering, and selection. */
export type LibraryRelease = CatalogRelease;

export type LibraryLegalDocumentLink = Readonly<{
  legal_document_id: string;
  kind: 'license' | 'notice';
  title: string;
  format: 'text' | 'pdf';
  file_name: string;
  content_url: string;
}>;

/** Fully resolved package projection returned by the desktop API. */
export type LibraryPackageSummary = Readonly<{
  package_id: string;
  vendor: string;
  technology: string;
  variant: string;
  display_name: string;
  release: LibraryRelease;
  target: LibraryTarget;
  primary_file_name: string;
  primary_sha256: string;
  primary_signature: Signature;
  legal_documents: readonly LibraryLegalDocumentLink[];
  size_bytes: number;
  availability: CatalogPackageAvailability;
  local_state: 'absent' | 'verified' | 'missing' | 'corrupt';
  automatic_selection_allowed: boolean;
}>;

export type LibraryPackagesOutput = Readonly<{
  packages: readonly LibraryPackageSummary[];
  catalog_status: 'active' | 'local_fallback';
}>;

export type LibraryPackageMutation = Readonly<{
  package_id: string;
  package: LibraryPackageSummary | null;
}>;

export type LibraryPackageState = Readonly<{
  package_id: string;
  version: string;
  is_downloaded: boolean;
  artifact_id: string | null;
}>;
