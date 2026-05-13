import type {
  Candidate,
  CandidateGroup,
  ComponentFile,
  GraphicsComponent,
} from '@entities/component';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import {
  formatCompactLibraryLabel,
  isKnownLibrary,
  libraryVendorOrder,
  libraryVendorKey,
  vendorLabelForLibraryVendorKey,
  type LibraryVendorKey,
} from '@shared/graphics';
import {
  formatLabel,
  nvapiControlsByLibrary,
  type NvApiControl,
} from '@entities/component';
import { fileNameFromPath } from '@shared/path';
import { isDefined, isNonEmptyString } from '@shared/validation';

const UNKNOWN_VALUE = 'Unknown';

const NO_DETECTED_FILE_LABEL = 'No detected file';
const NO_DETECTED_FILE_PATH = 'No file recorded';

const NO_LOCAL_REPLACEMENTS_OPTION = {
  value: '',
  label: 'No local replacements',
  disabled: true,
};

export type InstalledOption = {
  value: string;
  label: string;
  path: string;
  version?: string | null;
  sha256?: string | null;
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

export type LibrarySection = {
  libraryKey: string;
  label: string;
  rows: ConfiguredComponentRow[];
  nvapiControls: NvApiControl[];
  nvapiOwnerId: string;
  totalCandidates: number;
};

export type VendorKey = LibraryVendorKey;

export type VendorBlock = {
  key: VendorKey;
  label: string;
  sections: LibrarySection[];
  totalFiles: number;
  totalCandidates: number;
};

export type GraphicsConfiguratorViewModel = {
  componentRows: ComponentConfiguratorRow[];
  configuredRows: ConfiguredComponentRow[];
  librarySections: LibrarySection[];
  vendorBlocks: VendorBlock[];
};

export function displayValue(value?: string | null): string {
  return normalizedText(value) ?? UNKNOWN_VALUE;
}

export function createGraphicsConfiguratorViewModel(
  gameDetails: GameDetails,
  selections: Record<string, string>,
  isBusy: boolean,
): GraphicsConfiguratorViewModel {
  const componentRows = buildComponentRows(gameDetails);
  const configuredRows = componentRows.map((row) => buildConfiguredRow(row, selections, isBusy));
  const librarySections = buildLibrarySections(configuredRows);
  const vendorBlocks = buildVendorBlocks(librarySections);

  return {
    componentRows,
    configuredRows,
    librarySections,
    vendorBlocks,
  };
}

export function updateArtifactSelection(
  selections: Record<string, string>,
  componentId: string,
  artifactId: string,
): Record<string, string> {
  return updateSelection(selections, componentId, artifactId);
}

export function updateNvapiSelection(
  selections: Record<string, string>,
  componentId: string,
  controlId: string,
  artifactId: string,
): Record<string, string> {
  return updateSelection(selections, selectionKey(componentId, controlId), artifactId);
}

export function shouldReplaceSelectionMap(
  current: Record<string, string>,
  next: Record<string, string>,
): boolean {
  return !sameSelectionMap(current, next);
}

export function buildComponentRows(gameDetails: GameDetails): ComponentConfiguratorRow[] {
  const candidateGroupsByComponentId = indexCandidateGroupsByComponentId(
    gameDetails.candidate_groups,
  );

  return gameDetails.components.filter(isVisibleComponent).map((component) => {
    const group = candidateGroupsByComponentId.get(component.id) ?? null;
    const installedOptions = buildInstalledOptions(component);
    const installedValue = resolveInstalledValue(group, installedOptions);

    return {
      component,
      group,
      installedOptions,
      installedValue,
      nvapiControls: nvapiControlsByLibrary[component.technology] ?? [],
    };
  });
}

export function buildConfiguredRow(
  row: ComponentConfiguratorRow,
  selections: Record<string, string>,
  isBusy: boolean,
): ConfiguredComponentRow {
  const selectedCandidate = findSelectedCandidate(row, selections);

  return {
    ...row,
    currentInstalled: findInstalledSelection(row),
    selectedCandidate,
    candidatePath: selectedCandidatePath(row, selectedCandidate),
    candidateSummary: selectedCandidateSummary(row, selectedCandidate),
    canBuildPlan: Boolean(selectedCandidate) && !isBusy,
  };
}

export function buildLibrarySections(rows: ConfiguredComponentRow[]): LibrarySection[] {
  const sectionsByLibrary = new Map<string, LibrarySection>();

  for (const row of rows) {
    const section = getOrCreateLibrarySection(sectionsByLibrary, row);

    section.rows.push(row);
    section.totalCandidates += candidateCount(row.group);
  }

  return Array.from(sectionsByLibrary.values());
}

export function buildVendorBlocks(sections: LibrarySection[]): VendorBlock[] {
  const blocksByVendor = buildVendorBlockMap();

  for (const section of sections) {
    const vendorKey = libraryVendorKey(section.libraryKey);
    const vendorBlock = blocksByVendor.get(vendorKey);

    if (!vendorBlock) {
      continue;
    }

    vendorBlock.sections.push(section);
    vendorBlock.totalFiles += section.rows.length;
    vendorBlock.totalCandidates += section.totalCandidates;
  }

  return libraryVendorOrder
    .map((vendorKey) => blocksByVendor.get(vendorKey))
    .filter(isDefined)
    .filter(shouldShowVendorBlock);
}

export function sameSelectionMap(
  current: Record<string, string>,
  next: Record<string, string>,
): boolean {
  const currentEntries = Object.entries(current);
  const nextKeys = Object.keys(next);

  if (currentEntries.length !== nextKeys.length) {
    return false;
  }

  return currentEntries.every(([key, value]) => next[key] === value);
}

export function reconcileArtifactSelections(
  rows: ComponentConfiguratorRow[],
  currentSelections: Record<string, string>,
  activePlan: SwapPlan | null,
): Record<string, string> {
  const nextSelections: Record<string, string> = {};

  for (const row of rows) {
    const nextSelection = resolveArtifactSelection(row, currentSelections, activePlan);

    if (nextSelection) {
      nextSelections[row.component.id] = nextSelection;
    }
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

      nextSelections[key] = resolveNvapiSelection(control, currentSelections[key]);
    }
  }

  return nextSelections;
}

export function selectionKey(componentId: string, controlId: string): string {
  return `${componentId}::${controlId}`;
}

function updateSelection(
  selections: Record<string, string>,
  key: string,
  value: string,
): Record<string, string> {
  return {
    ...selections,
    [key]: value,
  };
}

export function installedOptionsForRow(row: ConfiguredComponentRow): {
  value: string;
  label: string;
}[] {
  return row.installedOptions.map(({ value, label }) => ({
    value,
    label,
  }));
}

export function candidateOptionsForRow(row: ConfiguredComponentRow): {
  value: string;
  label: string;
  disabled?: boolean;
}[] {
  const candidates = row.group?.candidates ?? [];

  if (candidates.length === 0) {
    return [NO_LOCAL_REPLACEMENTS_OPTION];
  }

  return candidates.map((candidate) => ({
    value: candidate.artifact_id,
    label: candidateOptionLabel(candidate),
  }));
}

function indexCandidateGroupsByComponentId(
  candidateGroups: CandidateGroup[],
): Map<string, CandidateGroup> {
  return new Map(candidateGroups.map((group) => [group.component_id, group]));
}

function buildInstalledOptions(component: GraphicsComponent): InstalledOption[] {
  if (component.files.length === 0) {
    return [buildMissingInstalledOption(component.id)];
  }

  return component.files.map(buildInstalledOption);
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

function buildMissingInstalledOption(componentId: string): InstalledOption {
  return {
    value: `missing:${componentId}`,
    label: NO_DETECTED_FILE_LABEL,
    path: NO_DETECTED_FILE_PATH,
    version: null,
    sha256: null,
  };
}

function resolveInstalledValue(
  group: CandidateGroup | null,
  installedOptions: InstalledOption[],
): string {
  if (group?.file_path && installedOptions.some((option) => option.path === group.file_path)) {
    return group.file_path;
  }

  return installedOptions[0]?.value ?? UNKNOWN_VALUE;
}

function getOrCreateLibrarySection(
  sectionsByLibrary: Map<string, LibrarySection>,
  row: ConfiguredComponentRow,
): LibrarySection {
  const libraryKey = row.component.technology;
  const existingSection = sectionsByLibrary.get(libraryKey);

  if (existingSection) {
    return existingSection;
  }

  const section: LibrarySection = {
    libraryKey,
    label: formatCompactLibraryLabel(libraryKey),
    rows: [],
    nvapiControls: row.nvapiControls,
    nvapiOwnerId: row.component.id,
    totalCandidates: 0,
  };

  sectionsByLibrary.set(libraryKey, section);

  return section;
}

function buildVendorBlockMap(): Map<VendorKey, VendorBlock> {
  return new Map(
    libraryVendorOrder.map((vendorKey) => [
      vendorKey,
      {
        key: vendorKey,
        label: vendorLabelForLibraryVendorKey(vendorKey),
        sections: [],
        totalFiles: 0,
        totalCandidates: 0,
      },
    ]),
  );
}

function shouldShowVendorBlock(block: VendorBlock): boolean {
  return block.key !== 'other' || block.sections.length > 0;
}

function isVisibleComponent(component: GraphicsComponent): boolean {
  return isKnownLibrary(component.technology);
}

function resolveArtifactSelection(
  row: ComponentConfiguratorRow,
  currentSelections: Record<string, string>,
  activePlan: SwapPlan | null,
): string | null {
  const group = row.group;

  if (!group || group.candidates.length === 0) {
    return null;
  }

  const candidateIds = new Set(group.candidates.map((candidate) => candidate.artifact_id));
  const plannedArtifactId = resolvePlannedArtifactId(group, candidateIds, activePlan);

  if (plannedArtifactId) {
    return plannedArtifactId;
  }

  const currentArtifactId = currentSelections[row.component.id];

  if (currentArtifactId && candidateIds.has(currentArtifactId)) {
    return currentArtifactId;
  }

  return group.candidates[0].artifact_id;
}

function resolvePlannedArtifactId(
  group: CandidateGroup,
  candidateIds: Set<string>,
  activePlan: SwapPlan | null,
): string | null {
  if (!activePlan) {
    return null;
  }

  const matchesCurrentGroup = activePlan.target_path === group.file_path;
  const matchesKnownCandidate = candidateIds.has(activePlan.artifact_id);

  return matchesCurrentGroup && matchesKnownCandidate ? activePlan.artifact_id : null;
}

function resolveNvapiSelection(control: NvApiControl, currentValue?: string): string {
  if (
    currentValue !== undefined &&
    control.options.some((option) => option.value === currentValue)
  ) {
    return currentValue;
  }

  return control.defaultValue;
}

function candidateCount(group: CandidateGroup | null): number {
  return group?.candidates.length ?? 0;
}

function candidateOptionLabel(candidate: Candidate): string {
  const version = normalizedText(candidate.version);

  return version ? `v${version}` : 'Version unknown';
}

function findSelectedCandidate(
  row: ComponentConfiguratorRow,
  selections: Record<string, string>,
): Candidate | null {
  const artifactId = selections[row.component.id];

  return row.group?.candidates.find((candidate) => candidate.artifact_id === artifactId) ?? null;
}

function findInstalledSelection(row: ComponentConfiguratorRow): InstalledOption {
  const selectedOption = row.installedOptions.find((option) => option.value === row.installedValue);

  if (selectedOption) {
    return selectedOption;
  }

  if (row.installedOptions.length > 0) {
    return row.installedOptions[0];
  }

  return buildMissingInstalledOption(row.component.id);
}

function selectedCandidatePath(row: ComponentConfiguratorRow, candidate: Candidate | null): string {
  if (candidate) {
    return candidate.file_path;
  }

  return row.group
    ? 'Choose a local replacement to stage a file swap plan.'
    : 'No compatible local replacements were found for this component.';
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

  return joinSummaryParts([
    formatLabel(candidate.comparison),
    versionSummary(candidate.version),
    warningSummary(candidate.warning),
  ]);
}

function versionSummary(version?: string | null): string | null {
  const normalizedVersion = normalizedText(version);

  return normalizedVersion ? `v${normalizedVersion}` : null;
}

function warningSummary(warning?: string | null): string | null {
  const normalizedWarning = normalizedText(warning);

  return normalizedWarning ? formatLabel(normalizedWarning) : null;
}

function joinSummaryParts(parts: (string | null | undefined)[]): string {
  return parts.filter(isNonEmptyString).join(' · ');
}

function normalizedText(value?: string | null): string | null {
  if (!isNonEmptyString(value)) {
    return null;
  }

  return value.trim();
}
