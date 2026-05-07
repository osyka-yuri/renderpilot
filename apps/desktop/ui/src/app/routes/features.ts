import type { Screen } from './screen';

export type FeatureAvailabilityContext = {
  selectedGameId: string | null;
  advancedMode: boolean;
};

export type FeatureItem = {
  id: string;
  title: string;
  description: string;
  screen: Screen;
  enabled: boolean;
  hidden: boolean;
};

type FeatureDefinition = {
  id: string;
  title: string;
  description: string;
  screen: Screen;
  requiresGameSelection?: boolean;
  requiresAdvancedMode?: boolean;
};

const FEATURE_DEFINITIONS: FeatureDefinition[] = [
  {
    id: 'games',
    title: 'Dashboard',
    description: 'Game cards, update status, risk, and primary actions.',
    screen: 'games',
  },
  {
    id: 'details',
    title: 'Game Details',
    description: 'Overview, libraries, streamlines, backups, and history.',
    screen: 'details',
    requiresGameSelection: true,
  },
  {
    id: 'library',
    title: 'Library',
    description: 'Artifacts, trust levels, and import sources.',
    screen: 'library',
  },
  {
    id: 'profiles',
    title: 'Profiles',
    description: 'Recommended presets and capability-aware templates.',
    screen: 'profiles',
  },
  {
    id: 'backups',
    title: 'Backups',
    description: 'Snapshot coverage, manifests, and restore readiness.',
    screen: 'backups',
  },
  {
    id: 'operations',
    title: 'History',
    description: 'Journaled operations, recovery hints, and rollback.',
    screen: 'operations',
  },
  {
    id: 'settings',
    title: 'Settings',
    description: 'Theme, language, scan paths, and advanced mode.',
    screen: 'settings',
  },
];

export function resolveFeatureItems(context: FeatureAvailabilityContext): FeatureItem[] {
  return FEATURE_DEFINITIONS.map((feature) => ({
    id: feature.id,
    title: feature.title,
    description: feature.description,
    screen: feature.screen,
    enabled: !feature.requiresGameSelection || !!context.selectedGameId,
    hidden: !!feature.requiresAdvancedMode && !context.advancedMode,
  }));
}
