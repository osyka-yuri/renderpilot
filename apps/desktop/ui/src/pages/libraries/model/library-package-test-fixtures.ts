import type {
  LibraryLegalDocumentLink,
  LibraryPackageSummary,
  ReleaseChannel,
  Signature,
} from '@entities/library';

export type PackageFixture = Readonly<{
  id: string;
  vendor?: LibraryPackageSummary['vendor'];
  technology?: string;
  variant?: string;
  version?: string;
  channel?: ReleaseChannel;
  architecture?: 'X86' | 'X64';
  compatibilityVersion?: number;
  label?: string | null;
  displayName?: string;
  signature?: Signature;
  legalDocuments?: LibraryPackageSummary['legal_documents'];
  sizeBytes?: number;
  isDownloaded?: boolean;
}>;

export function packagesOf(specs: readonly PackageFixture[]): LibraryPackageSummary[] {
  return specs.map((spec, index) => packageSummary(spec, index));
}

export function packageSummary(spec: PackageFixture, index = 0): LibraryPackageSummary {
  const digest = (index + 1).toString(16).padStart(64, '0');
  const vendorId = spec.vendor ?? 'nvidia';
  return {
    package_id: spec.id,
    artifact_id: `catalog:package-revision:${digest}`,
    vendor: vendorId,
    technology: spec.technology ?? 'dlss_super_resolution',
    variant: spec.variant ?? 'runtime',
    display_name: spec.displayName ?? `Package ${spec.id}`,
    release: {
      version: spec.version ?? '1.0.0',
      channel: spec.channel ?? 'stable',
      label: spec.label ?? null,
    },
    target: {
      os: 'windows',
      architecture: spec.architecture ?? 'X64',
      ...(spec.compatibilityVersion === undefined
        ? {}
        : {
            compatibility: {
              kind: 'd3d12_sdk' as const,
              version: spec.compatibilityVersion,
            },
          }),
    },
    revision_sha256: digest,
    primary_file_name: `${spec.id}.dll`,
    primary_sha256: digest,
    primary_signature: spec.signature ?? { status: 'unsigned' },
    legal_documents: spec.legalDocuments ?? [],
    size_bytes: spec.sizeBytes ?? 1,
    is_downloaded: spec.isDownloaded ?? false,
  };
}

export function legalDocumentLink(
  overrides: Partial<LibraryLegalDocumentLink> = {},
): LibraryLegalDocumentLink {
  const sha256 = 'a'.repeat(64);
  return {
    legal_document_id: `license.${sha256}`,
    kind: 'license',
    title: 'SDK License',
    format: 'text',
    file_name: 'LICENSE.txt',
    content_url: `https://cdn.example.test/libraries/legal/sha256/${sha256}.txt`,
    ...overrides,
  };
}
