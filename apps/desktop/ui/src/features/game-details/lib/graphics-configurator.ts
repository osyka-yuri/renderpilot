import type {
  Candidate,
  CandidateGroup,
  ComponentFile,
  GameDetails,
  GraphicsComponent,
  SwapPlan,
} from '@shared/api/types';
import { formatLabel, formatTechnology } from '@shared/utils/presenters';

const UNKNOWN_VALUE = 'Unknown';

export type InstalledOption = {
  value: string;
  label: string;
  path: string;
  version?: string | null;
  sha256?: string | null;
};

export type NvApiControl = {
  id: string;
  label: string;
  description: string;
  defaultValue: string;
  options: Array<{
    value: string;
    label: string;
  }>;
};

export type ComponentConfiguratorRow = {
  component: GraphicsComponent;
  group: CandidateGroup | null;
  installedOptions: InstalledOption[];
  installedValue: string;
  nvapiControls: NvApiControl[];
};

export type ConfiguredComponentRow = ComponentConfiguratorRow & {
  currentInstalled: InstalledOption;
  selectedCandidate: Candidate | null;
  candidatePath: string;
  candidateSummary: string;
  canBuildPlan: boolean;
};

export type TechnologySection = {
  technologyKey: string;
  label: string;
  rows: ConfiguredComponentRow[];
  nvapiControls: NvApiControl[];
  nvapiOwnerId: string;
  totalCandidates: number;
};

export type VendorKey = 'nvidia' | 'amd' | 'intel' | 'other';

export type VendorBlock = {
  key: VendorKey;
  label: string;
  sections: TechnologySection[];
  totalFiles: number;
  totalCandidates: number;
};

const nvapiControlsByTechnology: Record<string, NvApiControl[]> = {
  DlssSuperResolution: [
    {
      id: 'preset',
      label: 'DLSS preset override',
      description:
        'Capability-based preset selection when the driver profile exposes a DLSS override.',
      defaultValue: 'safe_original',
      options: [
        { value: 'safe_original', label: 'Safe Original' },
        { value: 'quality', label: 'Quality' },
        { value: 'balanced', label: 'Balanced' },
        { value: 'performance', label: 'Performance' },
      ],
    },
    {
      id: 'indicator',
      label: 'DLSS indicator',
      description: 'Driver-side indicator/debug setting when exposed by the NVIDIA profile.',
      defaultValue: 'off',
      options: [
        { value: 'off', label: 'Off' },
        { value: 'on', label: 'On' },
      ],
    },
  ],
  DlssFrameGeneration: [
    {
      id: 'fg_profile',
      label: 'DLSS FG profile',
      description:
        'Frame Generation profile when the driver can override the game-controlled default.',
      defaultValue: 'game_controlled',
      options: [
        { value: 'game_controlled', label: 'Game Controlled' },
        { value: 'low_latency', label: 'Low Latency' },
        { value: 'stable_120hz', label: 'Stable 120 Hz' },
        { value: 'stable_144hz', label: 'Stable 144 Hz' },
      ],
    },
  ],
  DlssRayReconstruction: [
    {
      id: 'rr_profile',
      label: 'DLSS RR profile',
      description: 'Ray Reconstruction profile when the driver exposes a controllable path.',
      defaultValue: 'game_controlled',
      options: [
        { value: 'game_controlled', label: 'Game Controlled' },
        { value: 'quality', label: 'Quality' },
        { value: 'balanced', label: 'Balanced' },
        { value: 'safe_original', label: 'Safe Original' },
      ],
    },
  ],
};

const vendorBlueprints: Array<{
  key: VendorKey;
  label: string;
}> = [
  { key: 'nvidia', label: 'NVIDIA' },
  { key: 'amd', label: 'AMD' },
  { key: 'intel', label: 'Intel' },
  { key: 'other', label: 'Additional' },
];

export function displayValue(value?: string | null): string {
  return value ?? UNKNOWN_VALUE;
}

export function compactList(values: string[], emptyCopy: string, maxVisible = 3): string {
  if (values.length === 0) {
    return emptyCopy;
  }

  const visibleValues = values.slice(0, maxVisible);
  const remainingCount = values.length - visibleValues.length;
  const suffix = remainingCount > 0 ? ` +${remainingCount} more` : '';

  return `${visibleValues.join(' · ')}${suffix}`;
}

export function fileNameFromPath(path: string): string {
  const normalized = path.replace(/\\/g, '/');

  return normalized.split('/').pop() ?? path;
}

export function buildComponentRows(gameDetails: GameDetails): ComponentConfiguratorRow[] {
  const candidateGroupsByComponentId = new Map(
    gameDetails.candidate_groups.map((group) => [group.component_id, group]),
  );

  return gameDetails.components.map((component) => {
    const group = candidateGroupsByComponentId.get(component.id) ?? null;
    const installedOptions =
      component.files.length > 0
        ? component.files.map(buildInstalledOption)
        : [
            {
              value: `missing:${component.id}`,
              label: 'No detected file',
              path: 'No file recorded',
              version: null,
              sha256: null,
            },
          ];
    const installedValue =
      group?.file_path && installedOptions.some((option) => option.path === group.file_path)
        ? group.file_path
        : installedOptions[0].value;

    return {
      component,
      group,
      installedOptions,
      installedValue,
      nvapiControls: nvapiControlsByTechnology[component.technology] ?? [],
    };
  });
}

export function buildConfiguredRow(
  row: ComponentConfiguratorRow,
  selections: Record<string, string>,
  isBusy: boolean,
): ConfiguredComponentRow {
  const currentInstalled = installedSelection(row);
  const candidate = selectedCandidate(row, selections);

  return {
    ...row,
    currentInstalled,
    selectedCandidate: candidate,
    candidatePath: selectedCandidatePath(row, candidate),
    candidateSummary: selectedCandidateSummary(row, candidate),
    canBuildPlan: !!candidate && !isBusy,
  };
}

export function buildTechnologySections(rows: ConfiguredComponentRow[]): TechnologySection[] {
  const sectionsByTechnology = new Map<string, TechnologySection>();

  for (const row of rows) {
    const existing = sectionsByTechnology.get(row.component.technology);

    if (existing) {
      existing.rows.push(row);
      existing.totalCandidates += row.group?.candidates.length ?? 0;
      continue;
    }

    sectionsByTechnology.set(row.component.technology, {
      technologyKey: row.component.technology,
      label: formatTechnology(row.component.technology),
      rows: [row],
      nvapiControls: row.nvapiControls,
      nvapiOwnerId: row.component.id,
      totalCandidates: row.group?.candidates.length ?? 0,
    });
  }

  return Array.from(sectionsByTechnology.values());
}

export function buildVendorBlocks(sections: TechnologySection[]): VendorBlock[] {
  const blocksByVendor = new Map<VendorKey, VendorBlock>(
    vendorBlueprints.map((blueprint) => [
      blueprint.key,
      {
        key: blueprint.key,
        label: blueprint.label,
        sections: [],
        totalFiles: 0,
        totalCandidates: 0,
      },
    ]),
  );

  for (const section of sections) {
    const vendorKey = vendorKeyForTechnology(section.technologyKey);
    const vendorBlock = blocksByVendor.get(vendorKey);

    if (!vendorBlock) {
      continue;
    }

    vendorBlock.sections.push(section);
    vendorBlock.totalFiles += section.rows.length;
    vendorBlock.totalCandidates += section.totalCandidates;
  }

  return vendorBlueprints
    .map((blueprint) => blocksByVendor.get(blueprint.key))
    .filter((block): block is VendorBlock => !!block)
    .filter((block) => block.key !== 'other' || block.sections.length > 0);
}

export function sameSelectionMap(
  current: Record<string, string>,
  next: Record<string, string>,
): boolean {
  const currentKeys = Object.keys(current);
  const nextKeys = Object.keys(next);

  if (currentKeys.length !== nextKeys.length) {
    return false;
  }

  return nextKeys.every((key) => current[key] === next[key]);
}

export function reconcileArtifactSelections(
  rows: ComponentConfiguratorRow[],
  currentSelections: Record<string, string>,
  activePlan: SwapPlan | null,
): Record<string, string> {
  const nextSelections: Record<string, string> = {};

  for (const row of rows) {
    if (!row.group || row.group.candidates.length === 0) {
      continue;
    }

    const candidateIds = new Set(row.group.candidates.map((candidate) => candidate.artifact_id));
    const currentValue = currentSelections[row.component.id];
    const plannedValue =
      activePlan &&
      activePlan.target_path === row.group.file_path &&
      candidateIds.has(activePlan.artifact_id)
        ? activePlan.artifact_id
        : null;

    nextSelections[row.component.id] =
      plannedValue ??
      (currentValue && candidateIds.has(currentValue)
        ? currentValue
        : row.group.candidates[0].artifact_id);
  }

  return nextSelections;
}

export function reconcileNvapiSelections(
  rows: ComponentConfiguratorRow[],
  currentSelections: Record<string, string>,
): Record<string, string> {
  const nextSelections: Record<string, string> = {};

  for (const row of rows) {
    for (const control of row.nvapiControls) {
      const key = selectionKey(row.component.id, control.id);
      const currentValue = currentSelections[key];
      const hasCurrentOption = control.options.some((option) => option.value === currentValue);

      nextSelections[key] = hasCurrentOption ? currentValue : control.defaultValue;
    }
  }

  return nextSelections;
}

export function selectionKey(componentId: string, controlId: string): string {
  return `${componentId}::${controlId}`;
}

export function installedOptionsForRow(row: ConfiguredComponentRow): Array<{
  value: string;
  label: string;
}> {
  return row.installedOptions.map((option) => ({
    value: option.value,
    label: option.label,
  }));
}

export function candidateOptionsForRow(row: ConfiguredComponentRow): Array<{
  value: string;
  label: string;
  disabled?: boolean;
}> {
  if (!row.group || row.group.candidates.length === 0) {
    return [{ value: '', label: 'No local replacements', disabled: true }];
  }

  return row.group.candidates.map((candidate) => ({
    value: candidate.artifact_id,
    label: candidateOptionLabel(candidate),
  }));
}

function buildInstalledOption(file: ComponentFile): InstalledOption {
  return {
    value: file.path,
    label: `${fileNameFromPath(file.path)} · v${displayValue(file.version)}`,
    path: file.path,
    version: file.version ?? null,
    sha256: file.sha256 ?? null,
  };
}

function vendorKeyForTechnology(technologyKey: string): VendorKey {
  const normalizedKey = technologyKey.toLowerCase();

  if (normalizedKey.startsWith('dlss') || normalizedKey.startsWith('nvidia')) {
    return 'nvidia';
  }

  if (normalizedKey.startsWith('amd')) {
    return 'amd';
  }

  if (normalizedKey.startsWith('intel')) {
    return 'intel';
  }

  return 'other';
}

function candidateOptionLabel(candidate: Candidate): string {
  return candidate.version ? `v${candidate.version}` : 'Version unknown';
}

function selectedCandidate(
  row: ComponentConfiguratorRow,
  selections: Record<string, string>,
): Candidate | null {
  const artifactId = selections[row.component.id];

  return row.group?.candidates.find((candidate) => candidate.artifact_id === artifactId) ?? null;
}

function installedSelection(row: ComponentConfiguratorRow): InstalledOption {
  return (
    row.installedOptions.find((option) => option.value === row.installedValue) ??
    row.installedOptions[0]
  );
}

function selectedCandidatePath(row: ComponentConfiguratorRow, candidate: Candidate | null): string {
  if (!candidate) {
    return row.group
      ? 'Choose a local replacement to stage a file swap plan.'
      : 'No compatible local replacements were found for this component.';
  }

  return candidate.file_path;
}

function selectedCandidateSummary(
  row: ComponentConfiguratorRow,
  candidate: Candidate | null,
): string {
  if (!candidate) {
    return row.group
      ? 'Current DLL is visible on the left, and the replacement list stays on the right.'
      : 'This detected component has no local DLL replacement candidates yet.';
  }

  const summaryParts = [formatLabel(candidate.comparison)];

  if (candidate.version) {
    summaryParts.push(`v${candidate.version}`);
  }

  if (candidate.warning) {
    summaryParts.push(formatLabel(candidate.warning));
  }

  return summaryParts.join(' · ');
}
