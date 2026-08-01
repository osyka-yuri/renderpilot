import { formatLabel } from '@entities/component';
import type { GameSummary } from '@entities/game';
import {
  formatOperationLabel,
  getCompletedDurationText,
  statusBadgeVariant,
  type OperationBadgeVariant,
  type OperationSummary,
} from '@entities/operation';
import { formatLocalDateTime } from '@shared/format';
import { t, type Locale } from '@shared/i18n';

const COMPONENT_ID_LABEL_MAP: Record<string, string> = {
  'dlss super resolution': 'DLSS Super Resolution',
  'dlss frame generation': 'DLSS Frame Generation',
  'dlss ray reconstruction': 'DLSS Ray Reconstruction',
  streamline: 'NVIDIA Streamline',
  fsr: 'AMD FSR',
};

export type OperationHistoryDetails = {
  operations: readonly OperationSummary[];
};

export type OperationViewModel = {
  id: string;
  kindLabel: string;
  statusLabel: string;
  badgeVariant: OperationBadgeVariant;
  createdAtText: string;
  itemCount: number;
  completedDurationText: string | null;
  ariaLabel: string;
  gameName: string;
  libraryType: string;
  fromVersion: string | null;
  toVersion: string | null;
};

export function createOperationViewModel(
  operation: OperationSummary,
  locale: Locale,
  gameCard?: GameSummary | null,
): OperationViewModel {
  const kindLabel = formatOperationLabel(operation.kind);
  const statusLabel = formatOperationLabel(operation.status);
  const createdAtText = formatLocalDateTime(operation.created_at, locale) ?? t('common.unknown');
  const badgeVariant = statusBadgeVariant(operation.status);

  const gameName = operation.metadata?.game_name ?? gameCard?.title ?? '-';

  let libraryType = '-';
  if (operation.metadata?.technology) {
    libraryType = formatLabel(operation.metadata.technology);
  } else if (operation.component_id) {
    // Fallback for old manual operations where technology is missing.
    const id = operation.component_id.toLowerCase();
    const match = Object.entries(COMPONENT_ID_LABEL_MAP).find(([key]) => id.includes(key));
    libraryType = match ? match[1] : formatLabel(operation.component_id);
  }

  return {
    id: operation.operation_id,
    kindLabel,
    statusLabel,
    badgeVariant,
    createdAtText,
    itemCount: operation.item_count,
    completedDurationText: getCompletedDurationText(
      operation.created_at,
      operation.completed_at ?? null,
    ),
    ariaLabel: t('operation.itemAria', {
      kind: kindLabel,
      status: statusLabel,
    }),
    gameName,
    libraryType,
    fromVersion: operation.metadata?.from_version ?? null,
    toVersion: operation.metadata?.to_version ?? null,
  };
}
