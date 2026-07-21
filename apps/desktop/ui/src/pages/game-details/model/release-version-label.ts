/** Formats backend-owned release identity without technology-specific inference. */
export function formatReleaseVersionLabel({
  version,
  releaseLabel,
  isDebug,
  unknownLabel,
}: Readonly<{
  version: string | null | undefined;
  releaseLabel: string | null | undefined;
  isDebug: boolean;
  unknownLabel: string;
}>): string {
  if (!version) {
    return unknownLabel;
  }

  return `v${version}${releaseLabel ? ` (${releaseLabel})` : ''}${isDebug ? ' (Debug)' : ''}`;
}
