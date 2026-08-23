/** Formats backend-owned release identity without technology-specific inference. */
export function formatReleaseVersionLabel({
  version,
  releaseLabel,
  unknownLabel,
}: Readonly<{
  version: string | null | undefined;
  releaseLabel: string | null | undefined;
  unknownLabel: string;
}>): string {
  if (!version) {
    return unknownLabel;
  }

  const label = releaseLabel?.trim();
  return label ? `v${version} (${label})` : `v${version}`;
}
