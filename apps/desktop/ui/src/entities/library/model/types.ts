export type BuildType = 'stable' | 'beta' | 'debug';

export type Signature = Readonly<{ status: 'signed'; signed_at: string } | { status: 'unsigned' }>;

export type LibraryManifest = Readonly<{
  schema_version: number;
  generated_at: string;
  entries: readonly LibraryManifestEntry[];
}>;

export type LibraryManifestEntry = Readonly<{
  entry_id: string;

  library: Readonly<{
    id: string;
    file_name: string;
  }>;

  version: Readonly<{
    value: string;
    sort_key: string;
  }>;

  build: Readonly<{
    type: BuildType;
    label: string | null;
  }>;

  files: Readonly<{
    dll: Readonly<{
      size_bytes: number;
      hashes: Readonly<{
        sha256: string;
      }>;
    }>;

    zip: Readonly<{
      size_bytes: number;
      download_url: string;
    }>;
  }>;

  signature: Signature;
}>;

export type LibraryState = Readonly<{
  id: string;
  version: string;
  is_downloaded: boolean;
  local_path: string | null;
  artifact_id: string | null;
}>;
