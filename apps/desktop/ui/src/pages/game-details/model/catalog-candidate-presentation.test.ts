import { describe, expect, it } from 'vitest';

import { candidate } from './candidate-group-fixtures';
import { presentCatalogCandidateOption } from './catalog-candidate-presentation';

describe('presentCatalogCandidateOption', () => {
  it('shows the complete composite release with the primary version first', () => {
    const value = candidate('1.3.7', {
      release_label: 'legacy artifact label',
      catalog_package: {
        package_id: 'xiph:vorbis',
        release: {
          version: '1.3.7',
          channel: 'stable',
          label: null,
          components: { ogg: '1.3.5', vorbis: '1.3.7' },
        },
        availability: 'available',
        automatic_selection_allowed: true,
        presentation: {
          variant: 'shared.lib',
          architecture: 'X64',
          unsigned: true,
          provenance: {
            kind: 'source_build',
            sources: {
              ogg: {
                repository: 'xiph/ogg',
                version: '1.3.5',
                tag: 'v1.3.5',
                tag_object_sha: null,
                commit_sha: '0123456789abcdef0123456789abcdef01234567',
                archive_url: 'https://example.test/ogg.tar.gz',
                archive_sha256: 'a'.repeat(64),
              },
              vorbis: {
                repository: 'xiph/vorbis',
                version: '1.3.7',
                tag: null,
                tag_object_sha: null,
                commit_sha: null,
                archive_url: 'https://example.test/vorbis.tar.gz',
                archive_sha256: 'b'.repeat(64),
              },
            },
            build_revision: 1,
            recipe_sha256: 'c'.repeat(64),
            verification_policy_sha256: 'd'.repeat(64),
            patches: {},
            toolchain: {
              runner_image: 'windows-2025',
              compiler: 'MSVC 19.44',
              linker: 'link.exe 14.44',
              windows_sdk: '10.0.26100.0',
              cmake: '4.1.1',
            },
          },
          legal_documents: [
            {
              legal_document_id: 'xiph-license',
              kind: 'license',
              title: 'Xiph BSD License',
              format: 'text',
              file_name: 'COPYING',
              content_url: 'https://example.test/COPYING',
            },
          ],
        },
      },
    });

    expect(
      presentCatalogCandidateOption(value, {
        unknown: 'Unknown',
      }),
    ).toEqual({
      versionLabel: 'v1.3.7',
      componentVersions: ['Vorbis 1.3.7', 'Ogg 1.3.5'],
    });
  });
});
