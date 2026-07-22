export type ReleaseChannel = 'stable' | 'beta' | 'debug';

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

export type LibraryRelease = Readonly<{
  /** Canonical package version used for presentation, ordering, and selection. */
  version: string;
  channel: ReleaseChannel;
  /** Optional supplemental annotation displayed verbatim after the version. */
  label: string | null;
}>;

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
  artifact_id: string;
  vendor: string;
  technology: string;
  variant: string;
  display_name: string;
  release: LibraryRelease;
  target: LibraryTarget;
  revision_sha256: string;
  primary_file_name: string;
  primary_sha256: string;
  primary_signature: Signature;
  legal_documents: readonly LibraryLegalDocumentLink[];
  size_bytes: number;
  is_downloaded: boolean;
}>;

export type LibraryPackageState = Readonly<{
  package_id: string;
  version: string;
  is_downloaded: boolean;
  artifact_id: string | null;
}>;
