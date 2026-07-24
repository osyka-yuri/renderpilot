import { publishSuccessNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { formatRestoredFilesSummary, formatUpdatedFilesSummary } from './presenters';
import type { ExecutedD3d12ExecutableAction } from './types';

export function publishApplyCompletedNotification(
  itemCount: number,
  executableAction?: ExecutedD3d12ExecutableAction | null,
): string {
  const shouldDescribeExecutableChange =
    executableAction !== null &&
    executableAction !== undefined &&
    (executableAction.kind === 'restore' ||
      executableAction.from_sdk_version === executableAction.original_sdk_version);

  if (!shouldDescribeExecutableChange) {
    return publishSuccessNotification(
      t('notify.applyCompleted'),
      formatUpdatedFilesSummary(itemCount),
    );
  }

  return publishSuccessNotification(
    executableAction.kind === 'restore'
      ? t('gameDetails.d3d12.action.restore', {
          from: executableAction.from_sdk_version,
          to: executableAction.to_sdk_version,
        })
      : t('gameDetails.d3d12.action.patch', {
          from: executableAction.from_sdk_version,
          to: executableAction.to_sdk_version,
        }),
    executableAction.executable_path,
  );
}

export function publishRollbackCompletedNotification(itemCount: number): string {
  return publishSuccessNotification(
    t('notify.rollbackCompleted'),
    formatRestoredFilesSummary(itemCount),
  );
}
