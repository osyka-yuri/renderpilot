import type { MessageDictionary } from './types';

/**
 * English catalog — the source of truth for the key set.
 *
 * Keys are flat and dot-grouped by feature area. Every other locale must
 * provide the exact same keys (enforced via `Record<MessageKey, …>`), so
 * adding a key here turns missing translations into type errors.
 */
export const en = {
  // ── App shell / navigation ──
  'nav.games': 'Games',
  'nav.libraries': 'Libraries',
  'nav.settings': 'Settings',
  'nav.operations': 'Operations',
  'nav.gameFallback': 'Game',
  'shell.refresh': 'Refresh',

  // ── Settings: appearance section ──
  'settings.appearance.title': 'Appearance',
  'settings.appearance.description': 'Customize the application look and language.',
  'settings.appearance.theme.title': 'Theme',
  'settings.appearance.theme.description': 'Choose a color theme for the application.',
  'settings.appearance.theme.triggerLabel': 'Theme',
  'settings.appearance.theme.placeholder': 'Select theme',
  'settings.appearance.language.title': 'Language',
  'settings.appearance.language.description': 'Select the interface language.',
  'settings.appearance.language.triggerLabel': 'Language',
  'settings.appearance.language.placeholder': 'Select language',

  // ── Settings: theme options ──
  'settings.theme.system': 'System',
  'settings.theme.dark': 'Dark',
  'settings.theme.light': 'Light',

  // ── Settings: language options (en/ru labels are endonyms — identical in every locale) ──
  'settings.language.system': 'System default',
  'settings.language.en': 'English',
  'settings.language.ru': 'Русский',
  'settings.language.es': 'Español',
  'settings.language.zh': '中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  // ── Settings: tabs ──
  'settings.tabs.appearance': 'Appearance',
  'settings.tabs.catalog': 'Catalog',

  // ── Game card ──
  'game.card.action.details': 'Details',
  'game.card.action.journal': 'Journal',
  'game.card.action.detailsAria': 'Open details for {title}',
  'game.card.action.journalAria': 'Open journal for {title}',
  'game.card.detectedLibraries': 'Detected components',
  'game.card.badge.upToDate': 'Up to date',
  'game.card.badge.updatesAvailable': 'Updates available',
  'game.card.badge.updatesAvailableCount': {
    one: '1 update available',
    other: '{count} updates available',
  },
  'game.libraries.empty': 'No components found',

  // ── Game cover ──
  'game.cover.alt': 'Cover',
  'game.cover.altWithTitle': 'Cover: {title}',
  'game.cover.menu.ariaLabel': 'Cover options for {title}',
  'game.cover.menu.fetch': 'Download cover',
  'game.cover.menu.fetching': 'Downloading…',
  'game.cover.menu.fetchHint': 'Search for a cover online.',
  'game.cover.menu.pick': 'Choose image file…',
  'game.cover.menu.pickHint': 'Select a local image to use as a cover.',
  'game.cover.menu.clear': 'Remove cover',
  'game.cover.menu.clearHint': 'Restore the default cover.',

  // ── Games dashboard summary ──
  'game.dashboard.summary': 'Dashboard',
  'game.dashboard.games': { one: '{count} game', other: '{count} games' },
  'game.dashboard.updates': { one: '{count} update', other: '{count} updates' },
  'game.dashboard.rollbacksReady': {
    one: '{count} rollback available',
    other: '{count} rollbacks available',
  },

  // ── Elevation banner ──
  'elevation.title': 'Administrator privileges required',
  'elevation.description': 'Some settings cannot be changed without administrator rights.',
  'elevation.relaunch': 'Restart as administrator',
  'elevation.relaunchFailed': 'Could not restart as administrator',
  'elevation.dismiss': 'Dismiss',

  // ── Games page / catalog ──
  'games.scanFolder': 'Scan Folder',
  'games.scanning': 'Scanning...',
  'games.libraryActions': 'Actions',
  'games.search': 'Search games',
  'games.openFilters': 'Filters',
  'games.openFiltersActive': 'Filters (active)',
  'games.loading': 'Loading...',
  'games.empty.title': 'No games found',
  'games.empty.description': 'Scan a folder to add games to the dashboard.',
  'games.filterEmpty.title': 'No matches found',
  'games.filterEmpty.description': 'Try changing your search or filters.',
  'games.filterEmpty.reset': 'Reset Filters',

  // ── Settings: catalog (cover sources) ──
  'settings.catalog.title': 'Cover sources',
  'settings.catalog.description': 'Select online sources for downloading game covers.',
  'settings.catalog.steamKey.srLabel': 'SteamGridDB API key',
  'settings.catalog.steamKey.placeholder': 'API key',
  'settings.catalog.steamKey.loading': 'Loading…',
  'settings.catalog.steamKey.save': 'Save',
  'settings.catalog.steamKey.saved': 'Saved',
  'settings.catalog.steamKey.cleared': 'Cleared',
  'settings.catalog.steamKey.readError': 'Failed to read settings.',
  'settings.catalog.steamKey.saveError': 'Failed to save settings.',

  // ── Common ──
  'common.unknown': 'Unknown',

  // ── Game details: empty states ──
  'gameDetails.noGameSelected.title': 'No game selected',
  'gameDetails.noGameSelected.description': 'Select a game from the dashboard to view its details.',
  'gameDetails.noComponents.title': 'No components found',
  'gameDetails.noComponents.description':
    'This game does not have any supported graphics components.',

  // ── Game details: component version row ──
  'gameDetails.version.noReplacements': 'No alternative versions',
  'gameDetails.version.restoreOriginal': 'Restore original {fileName}',

  // ── Game details: vendor component card ──
  'gameDetails.vendor.description': 'Change the component version.',

  // ── Game details: DLSS component card ──
  'gameDetails.dlss.description': 'Change the DLSS version or override its settings.',
  'gameDetails.dlss.descriptionSwapOnly': 'Change the DLSS version.',
  'gameDetails.dlss.libraryFileLabel': 'File version',
  'gameDetails.dlss.driverOverridesLabel': 'NVIDIA profile overrides',
  'gameDetails.dlss.adminRequired': 'Restart the app as administrator to change these settings.',

  // ── Game details: Streamline card ──
  'gameDetails.streamline.description': 'Manage Streamline plugins.',
  'gameDetails.streamline.versionTitle': 'Global Streamline version',
  'gameDetails.streamline.versionDescription': 'Applies the same version to all plugins.',
  'gameDetails.streamline.noOtherVersions': 'No other versions',
  'gameDetails.streamline.mixed': 'Mixed versions',
  'gameDetails.streamline.updatesSummary': '{updates} updates · {missing} missing',
  'gameDetails.streamline.restoreAllAria': 'Restore all plugins to original',
  'gameDetails.streamline.restoreAllTooltip': 'Restore all to original',
  'gameDetails.streamline.mixedWarning':
    'Plugins are using different versions. Select a version above to sync them.',

  // ── Game details: NVIDIA profile card ──
  'gameDetails.profile.title': 'NVIDIA Profile',
  'gameDetails.profile.description': 'Configure NVIDIA driver settings for this game.',
  'gameDetails.profile.target': 'Executable file',
  'gameDetails.profile.loading': 'Loading...',
  'gameDetails.profile.pinnedManual': 'Manually selected.',
  'gameDetails.profile.autoDetected': 'Detected automatically.',
  'gameDetails.profile.noExeDetected': 'No executable found for this game.',
  'gameDetails.profile.noExe': 'No executable',
  'gameDetails.profile.autoDetect': 'Auto-detect',
  'gameDetails.profile.filteredTag': '(filtered)',
  'gameDetails.profile.filteredLabel': '{fileName} (filtered)',
  'gameDetails.profile.noProfile': 'NVIDIA profile not found. Launch the game once and try again.',

  // ── Game details: DLSS indicator card ──
  'gameDetails.indicator.title': 'DLSS Indicator',
  'gameDetails.indicator.description':
    'Show an overlay with the active DLSS version and settings during gameplay.',
  'gameDetails.indicator.systemWide': 'System-wide',
  'gameDetails.indicator.adminRequired': 'Restart the app as administrator to change this setting.',
  'gameDetails.indicator.overlayTitle': 'On-screen overlay',
  'gameDetails.indicator.overlayDescription': 'Applies to all games on this PC.',
  'gameDetails.indicator.toggleAria': 'Toggle DLSS indicator',

  // ── Game details: NVAPI setting row ──
  'gameDetails.nvapi.requiresDriver': 'requires driver {version}+',
  'gameDetails.nvapi.unavailable': 'unavailable',
  'gameDetails.nvapi.resetDefault': 'Reset to default',
  'gameDetails.nvapi.alreadyDefault': 'Already at default',
  'gameDetails.nvapi.restoreBaselineAria': 'Restore initial value',
  'gameDetails.nvapi.restoreBaseline': 'Restore initial value',
  'gameDetails.nvapi.alreadyBaseline': 'Already at initial value',
  'gameDetails.nvapi.noBaseline': 'No initial value saved',

  // ── Operations page ──
  'operations.title': 'History',
  'operations.subtitleAll': 'All activities',
  'operations.subtitleGame': 'Activity for {title}',
  'operations.viewGame': 'View Game',
  'operations.loading': 'Loading...',
  'operations.empty': 'No history yet',
  'operations.historyAria': 'Activity history',
  'operations.items': 'Items',

  // ── Libraries page ──
  'libraries.error': 'Error',
  'libraries.hash.copy': 'Copy Hash',
  'libraries.hash.copied': 'Copied',
  'libraries.hash.failed': 'Failed to copy',
  'libraries.hash.copiedToast': 'Hash copied to clipboard',
  'libraries.sort.asc': 'Sort ascending',
  'libraries.sort.desc': 'Sort descending',
  'libraries.sort.none': 'Not sorted',
  'libraries.actions.delete': 'Delete',
  'libraries.actions.download': 'Download',
  'libraries.actions.deletedToast': 'Deleted {version}',
  'libraries.actions.downloadedToast': 'Downloaded {version}',
  'libraries.actions.failedToast': 'Failed to {action}',

  // ── Common actions ──
  'common.cancel': 'Cancel',
  'common.apply': 'Apply',

  // ── Filter games ──
  'filters.title': 'Filters',
  'filters.launchers.title': 'Launchers',
  'filters.launchers.empty': 'No launchers found',
  'filters.launchers.reorder': 'Move {label}',
  'filters.libraries.title': 'Components',
  'filters.libraries.empty': 'No components found',

  // ── Operation presenters (status / kind / risk labels) ──
  'operation.label.low': 'Low risk',
  'operation.label.medium': 'Medium risk',
  'operation.label.high': 'High risk',
  'operation.label.blocked': 'Blocked',
  'operation.label.planned': 'Planned',
  'operation.label.completed': 'Completed',
  'operation.label.failed': 'Failed',
  'operation.label.rolledBack': 'Rolled Back',
  'operation.label.replaceComponent': 'Change Version',
  'operation.duration': 'Finished in {seconds}s',
  'operation.filesUpdated.none': 'No files updated.',
  'operation.filesUpdated.count': { one: '1 file updated.', other: '{count} files updated.' },
  'operation.filesRestored.none': 'No files restored.',
  'operation.filesRestored.count': { one: '1 file restored.', other: '{count} files restored.' },
  'operation.itemAria': '{kind}, {status}',

  // ── Notifications (toasts) ──
  'notify.stalePlan': 'The operation plan is outdated. Please try again.',
  'notify.missingStableGameId': 'Could not identify the game.',
  'notify.coverPickerPreview': 'Please use the desktop app to pick a cover.',
  'notify.coverUpdated.title': 'Cover updated',
  'notify.coverUpdated.body': 'Your custom cover has been saved.',
  'notify.coverDownloaded.title': 'Cover downloaded',
  'notify.coverDownloaded.body': 'The game cover has been updated.',
  'notify.coverRemoved.title': 'Cover removed',
  'notify.coverRemoved.body': 'Restored the default cover.',
  'notify.applyCompleted': 'Changes applied',
  'notify.rollbackCompleted': 'Rollback completed',
  'notify.statusError': 'Error',
  'notify.statusWarning': 'Warning',

  // ── Library scan ──
  'scan.partialWarning': {
    one: 'Could not scan 1 folder.',
    other: 'Could not scan {count} folders.',
  },

  // ── Background cover sync ──
  'coverSync.failed': 'Failed to sync covers.',
  'coverSync.refreshFailed': 'Failed to sync covers.',

  // ── NVIDIA driver context (toasts) ──
  'nvidia.adminRequired': 'Administrator privileges required',
  'nvidia.relaunchTo': 'Restart as administrator to {action}.',
  'nvidia.action.changeSetting': 'apply settings',
  'nvidia.action.revertSetting': 'revert settings',
  'nvidia.changeSettingFailed': 'Failed to apply settings',
  'nvidia.revertDefaultFailed': 'Failed to restore default settings',
  'nvidia.revertBaselineFailed': 'Failed to restore initial settings',
  'nvidia.setExeFailed': 'Failed to configure the executable',
  'nvidia.clearExeFailed': 'Failed to clear the executable configuration',

  // ── DLSS indicator context (toasts) ──
  'indicator.relaunchToToggle': 'Restart as administrator to toggle the DLSS indicator.',
  'indicator.changeFailed': 'Failed to toggle the DLSS indicator',

  // ── Libraries table ──
  'libraries.column.version': 'Version',
  'libraries.column.hash': 'Hash',
  'libraries.column.signed': 'Signed',
  'libraries.column.size': 'Size',
  'libraries.column.actions': 'Actions',
  'libraries.unsigned': 'Unsigned',
  'libraries.invalidDate': 'Invalid date',

  // ── Settings: cover source rows ──
  'settings.catalog.source.steam.aria': 'Download covers from Steam',
  'settings.catalog.source.steam.title': 'Steam',
  'settings.catalog.source.steam.description': 'Download covers from the public Steam catalog.',
  'settings.catalog.source.gog.aria': 'Download covers from GOG',
  'settings.catalog.source.gog.title': 'GOG',
  'settings.catalog.source.gog.description': 'Download covers from the official GOG catalog.',
  'settings.catalog.source.steamgriddb.aria': 'Download covers from SteamGridDB',
  'settings.catalog.source.steamgriddb.title': 'SteamGridDB',
  'settings.catalog.source.steamgriddb.description':
    'Download community covers from SteamGridDB. Requires an API key.',
  'settings.catalog.artworkReadError': 'Failed to load cover settings.',
  'settings.catalog.artworkSaveError': 'Failed to save cover settings.',

  // ── Backend user messages (mirror of src-tauri/commands/error/strings.rs) ──
  'user_message.invalid_argument': 'Invalid input provided.',
  'user_message.invalid_game_reference': 'Game not found.',
  'user_message.invalid_component_reference': 'Component not found.',
  'user_message.invalid_artifact_reference': 'Item not found.',
  'user_message.invalid_operation_reference': 'Action not found.',
  'user_message.missing_required_info': 'Missing required information.',
  'user_message.unexpected_input': 'Unexpected input received.',
  'user_message.unrecognized_option': 'Unknown option provided.',
  'user_message.unsupported_technology_filter': 'Unsupported filter.',
  'user_message.non_unicode_input': 'Text contains invalid characters.',
  'user_message.response_serialization_failed': 'Failed to process the request.',
  'user_message.plan_changed_rebuild': 'The task is outdated. Please try again.',
  'user_message.game_not_in_catalog': 'Game is not supported.',
  'user_message.operation_not_found': 'Action not found.',
  'user_message.artifact_not_found': 'Item not found.',
  'user_message.component_not_found': 'Component not found.',
  'user_message.invalid_operation_state': 'This action is currently unavailable.',
  'user_message.operation_could_not_complete': 'Failed to complete the action.',
  'user_message.command_task_failed': 'Failed to execute the command.',
  'user_message.steamgriddb_api_key_missing':
    'Please provide a SteamGridDB API key in the settings.',
  'user_message.unsupported_cover_image_type': 'Unsupported image format.',
  'user_message.cover_download_failed': 'Failed to download the cover.',
  'user_message.cover_artwork_not_found': 'No cover found for this game.',
  'user_message.cover_file_system_error': 'Failed to save the cover to disk.',
  'user_message.nvapi_requires_administrator':
    'Administrator rights are required to change this setting.',

  // ── Backend suggested actions ──
  'suggested_action.refresh_games': 'Refresh the games list and try again.',
  'suggested_action.reload_game_details': 'Refresh the game details and try again.',
  'suggested_action.refresh_candidates': 'Refresh the list and try again.',
  'suggested_action.rebuild_plan_or_reload_operations': 'Refresh the view and try again.',
  'suggested_action.retry_after_required_data': 'Please wait and try again later.',
  'suggested_action.reload_desktop': 'Restart the app and try again.',
  'suggested_action.normalize_text': 'Check your input and try again.',
  'suggested_action.inspect_logs': 'If the problem persists, try restarting the app.',
  'suggested_action.retry_or_restart': 'If the problem persists, try restarting the app.',
  'suggested_action.rebuild_operation_plan': 'Please restart the action.',
  'suggested_action.refresh_or_scan_game_folder': 'Refresh the list or scan the folder again.',
  'suggested_action.relaunch_as_administrator': 'Restart the app as administrator and try again.',
} satisfies MessageDictionary;

export type MessageKey = keyof typeof en;
