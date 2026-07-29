import { openFolderPicker } from '@shared/api';
import { t } from '@shared/i18n';

/** Opens the system folder picker for one game installation root. */
export async function selectGameInstallFolder(): Promise<string | null> {
  return openFolderPicker({ title: t('games.chooseInstallFolder') });
}
