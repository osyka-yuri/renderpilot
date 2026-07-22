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
  'nav.operations': 'Journal',
  'nav.gameFallback': 'Game',
  'nav.donate': 'Donate',
  'shell.refresh': 'Refresh',
  'shell.updateAvailable': 'Update available',

  // ── Settings: appearance section ──
  'settings.appearance.title': 'Appearance',
  'settings.appearance.description': 'Customize the application look and language.',
  'settings.appearance.theme.title': 'Theme',
  'settings.appearance.theme.description': 'Choose a color theme for the application.',
  'settings.appearance.theme.triggerLabel': 'Theme',
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
  'settings.tabs.general': 'General',
  'settings.tabs.renodx': 'RenoDX',
  'settings.tabs.catalog': 'Catalog',
  'settings.tabs.nvidia': 'NVIDIA',

  // ── Settings: NVIDIA ──
  'settings.nvidia.indicator.title': 'DLSS Indicator',
  'settings.nvidia.indicator.description':
    'Show an overlay with the active DLSS version and settings during gameplay.',
  'settings.nvidia.indicator.systemWide': 'System-wide',
  'settings.nvidia.indicator.adminRequired':
    'Restart the app as administrator to change this setting.',
  'settings.nvidia.indicator.overlayTitle': 'On-screen overlay',
  'settings.nvidia.indicator.overlayDescription': 'Applies to all games on this PC.',
  'settings.nvidia.indicator.toggleAria': 'Toggle DLSS indicator',
  'settings.nvidia.global.title': 'Global DLSS settings',
  'settings.nvidia.global.description':
    'Defaults applied to every game that has no game-specific override, via the NVIDIA base profile.',
  'settings.nvidia.global.systemWide': 'System-wide',
  'settings.nvidia.global.adminRequired':
    'Restart the app as administrator to change these settings.',
  'settings.nvidia.global.familySr': 'DLSS Super Resolution',
  'settings.nvidia.global.familyFg': 'DLSS Frame Generation',
  'settings.nvidia.global.familyRr': 'DLSS Ray Reconstruction',
  'settings.nvidia.unsupported.title': 'No NVIDIA GPU detected',
  'settings.nvidia.unsupported.description':
    'These settings require a supported NVIDIA graphics card.',

  // ── Game card ──
  'game.card.action.details': 'Details',
  'game.card.action.detailsAria': 'Open details for {title}',
  'game.card.detectedLibraries': 'Detected components',
  'game.card.availableAddons': 'Available add-ons',
  'game.card.badge.upToDate': 'Up to date',
  'game.card.badge.updatesAvailable': 'Updates available',
  'game.card.badge.updatesAvailableCount': {
    one: '1 update available',
    other: '{count} updates available',
  },
  'game.card.menu.ariaLabel': 'Options for {title}',
  'game.card.menu.favorite.add': 'Add to favorites',
  'game.card.menu.favorite.remove': 'Remove from favorites',
  'game.card.menu.favorite.toggleHint': 'Toggle favorite status for this game.',
  'game.card.menu.hidden.add': 'Hide game',
  'game.card.menu.hidden.remove': 'Unhide game',
  'game.card.menu.hidden.toggleHint': 'Toggle hidden status for this game.',

  // ── Game cover ──
  'game.cover.alt': 'Cover',
  'game.cover.altWithTitle': 'Cover: {title}',
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

  // ── Elevation banner ──
  'elevation.title': 'Administrator privileges required',
  'elevation.description': 'Some settings cannot be changed without administrator rights.',
  'elevation.relaunch': 'Restart as administrator',
  'elevation.relaunchFailed': 'Could not restart as administrator',
  'elevation.dismiss': 'Dismiss',
  'error.boundary.title': 'Something went wrong',
  'error.boundary.description':
    'This screen ran into an unexpected error. You can try again, or switch to another section.',
  'error.boundary.reset': 'Try again',

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
  'settings.catalog.steamKey.show': 'Show API key',
  'settings.catalog.steamKey.hide': 'Hide API key',
  'settings.catalog.steamKey.getKey': 'Get an API key',

  // ── Settings: RenoDX ──
  'settings.renodx.vulkan.description':
    'Manage the shared ReShade Vulkan layer used by Vulkan RenoDX games.',
  'settings.renodx.vulkan.channel': 'Vulkan layer channel',
  'settings.renodx.vulkan.channelDescription':
    'Choose which ReShade channel the shared Vulkan layer should use.',
  'settings.renodx.vulkan.loadError': 'Failed to load the Vulkan layer status.',
  'settings.renodx.vulkan.saveError': 'Failed to save the Vulkan layer channel.',
  'settings.renodx.vulkan.applyError': 'Failed to apply the Vulkan layer.',

  // ── Settings: about ──
  'settings.about.title': 'About',
  'settings.about.description': 'Check for app updates and version info.',
  'settings.about.version.title': 'App version',
  'settings.about.version.loading': 'Loading...',
  'settings.about.checkForUpdates': 'Check for updates',
  'settings.about.updateInProgress': 'Updating…',
  'settings.about.updateAvailable': 'Update available',
  'settings.about.upToDate': 'You are on the latest version',
  'settings.about.updateCheckError': 'Failed to check for updates',

  'settings.about.updateDialog.title': 'Update available',
  'settings.about.updateDialog.versionLine': '{currentVersion} → {version}',
  'settings.about.updateDialog.releaseDate': 'Released {date}',
  'settings.about.updateDialog.releaseNotes': 'Release notes',
  'settings.about.updateDialog.noNotes': 'No release notes were provided for this update.',
  'settings.about.updateDialog.notesTruncated': 'Release notes were shortened.',

  'settings.about.updateDialog.installAndRestart': 'Install and restart',
  'settings.about.updateDialog.later': 'Later',
  'settings.about.updateDialog.close': 'Close',
  'settings.about.updateDialog.retryDownload': 'Retry download',
  'settings.about.updateDialog.retryInstall': 'Retry installation',
  'settings.about.updateDialog.restartNow': 'Restart now',

  'settings.about.updateDialog.downloading': 'Downloading update…',
  'settings.about.updateDialog.downloadingBytes': '{received} downloaded',
  'settings.about.updateDialog.downloadingBytesTotal': '{received} of {total}',
  'settings.about.updateDialog.verifying': 'Verifying update…',
  'settings.about.updateDialog.verifyingDescription': 'Checking the downloaded package.',
  'settings.about.updateDialog.installing':
    'Installing update… The app will close; the installer may appear briefly.',
  'settings.about.updateDialog.restarting': 'Restarting application…',

  'settings.about.updateDialog.prepareErrorTitle': 'Download or verification failed',
  'settings.about.updateDialog.prepareErrorDescription':
    'The update could not be downloaded or verified. Check your connection and try again.',
  'settings.about.updateDialog.installErrorTitle': 'Installation failed',
  'settings.about.updateDialog.installErrorDescription':
    'The update could not be installed. You can try again.',
  'settings.about.updateDialog.restartRequiredTitle': 'Restart required',
  'settings.about.updateDialog.restartRequiredDescription':
    'The update was installed, but the application could not restart automatically. Restart RenderPilot manually to finish the update.',

  'settings.about.updateDialog.progressAria': 'Download progress: {percent} percent',
  'settings.about.updateDialog.indeterminateProgressAria': 'Download progress',

  // ── Common ──
  'common.unknown': 'Unknown',
  'common.downloadProgress': 'Download progress',

  // ── Game details: empty states ──
  'gameDetails.noGameSelected.title': 'No game selected',
  'gameDetails.noGameSelected.description': 'Select a game from the dashboard to view its details.',

  // ── Game details: component version row ──
  'gameDetails.version.noReplacements': 'No alternative versions',
  'gameDetails.version.restoreOriginal': 'Restore original {fileName}',
  'gameDetails.version.fileCount': { one: '1 file', other: '{count} files' },

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
  'gameDetails.streamline.mixedRange': 'Mixed versions (v{min} – v{max})',
  'gameDetails.streamline.updatesSummary': '{updates} updates · {missing} missing',
  'gameDetails.streamline.restoreAllAria': 'Restore all plugins to original',
  'gameDetails.streamline.restoreAllTooltip': 'Restore all to original',
  'gameDetails.updateAll.action': 'Update all to latest',
  'gameDetails.updateAll.actionCount': 'Update all to latest ({count})',
  'gameDetails.updateAll.upToDate': 'Everything is up to date',
  'gameDetails.updateAll.partialFailure':
    'Some updates failed ({count}). Check the details and try again.',
  'gameDetails.updateAll.tooltip': {
    one: 'Update 1 component to its latest version',
    other: 'Update {count} components to their latest versions',
  },
  // ── Game details: executable selector (shared) ──
  'gameDetails.executable.title': 'Game executable',
  'gameDetails.executable.description':
    'The game executable — the NVIDIA profile applies to it, and RenoDX installs into its folder.',
  'gameDetails.executable.detectedGroup': 'Detected game executables',
  'gameDetails.executable.otherGroup': 'Other (launchers, installers, tools)',
  'gameDetails.executable.customBadge': 'Custom',
  'gameDetails.executable.reset': 'Reset to auto-detect',
  'gameDetails.executable.resetConfirm': 'Discard your manual choice and use auto-detection?',
  'gameDetails.executable.tooltipAuto':
    'Game executable: auto-detected. Used by the NVIDIA profile and RenoDX.',
  'gameDetails.executable.tooltipCustom':
    'Game executable: manually selected. Used by the NVIDIA profile and RenoDX.',
  // ── Game details: NVIDIA profile card ──
  'gameDetails.profile.title': 'NVIDIA Profile',
  'gameDetails.profile.description': 'Configure NVIDIA driver settings for this game.',
  'gameDetails.profile.pinnedManual': 'Manually selected.',
  'gameDetails.profile.autoDetected': 'Detected automatically.',
  'gameDetails.profile.noExeDetected': 'No executable found for this game.',
  'gameDetails.profile.noExe': 'No executable',
  'gameDetails.profile.noProfile': 'NVIDIA profile not found.',

  // ── Game details: NVAPI setting row ──
  'gameDetails.nvapi.requiresDriver': 'requires driver {version}+',
  'gameDetails.nvapi.unavailable': 'unavailable',
  'gameDetails.nvapi.resetDefault': 'Reset to default',
  'gameDetails.nvapi.alreadyDefault': 'Already at default',
  'gameDetails.nvapi.restoreBaselineAria': 'Restore baseline',
  'gameDetails.nvapi.restoreBaseline': 'Restore baseline',
  'gameDetails.nvapi.alreadyBaseline': 'Already at baseline',
  'gameDetails.nvapi.noBaseline': 'No baseline saved',

  'gameDetails.nvapi.warning.noDll': 'No DLSS DLL detected in the install directory.',
  'gameDetails.nvapi.warning.noManifest': 'Manifest has no entry for this DLL version.',
  'gameDetails.nvapi.warning.noExecutable': 'No executable resolved for this game.',
  'gameDetails.nvapi.warning.nvapiUnavailable': 'NVAPI unavailable.',
  'gameDetails.nvapi.warning.nvapiInitFailed': 'NVAPI initialize failed.',
  'gameDetails.nvapi.warning.drsFailed': 'DRS session could not be created.',

  // ── Operations page ──
  'operations.title': 'Operations Journal',
  'operations.subtitleGame': 'Activity for {title}',
  'operations.loading': 'Loading...',
  'operations.empty': 'No history yet',
  'operations.gameName': 'Game',
  'operations.date': 'Date',
  'operations.status': 'Status',
  'operations.action': 'Action',
  'operations.libraryType': 'Library Type',
  'operations.version': 'Version',

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
  'libraries.actions.downloadAll': 'Download latest',
  'libraries.actions.downloadAllCount': 'Download latest ({count})',
  'libraries.actions.downloadAllUpToDate': 'All latest versions already downloaded',
  'libraries.actions.downloadAllTooltip': {
    one: 'Download 1 latest version',
    other: 'Download {count} latest versions',
  },
  'libraries.actions.downloadAllDoneToast': {
    one: 'Downloaded {count} library',
    other: 'Downloaded {count} libraries',
  },
  'libraries.actions.downloadAllPartialToast': 'Downloaded {succeeded}, {failed} failed',
  'libraries.actions.downloadAllNoneToast': 'All latest versions already downloaded',

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
  'filters.addons.title': 'Add-ons',

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
  'notify.favoriteFailed': 'Failed to change favorite status.',
  'notify.favoriteAdded': 'Added to favorites.',
  'notify.favoriteRemoved': 'Removed from favorites.',
  'notify.hiddenFailed': 'Failed to change hidden status.',
  'notify.gameHidden': 'Game hidden.',
  'notify.gameUnhidden': 'Game unhidden.',
  'notify.applyCompleted': 'Changes applied',
  'notify.rollbackCompleted': 'Rollback completed',
  'notify.swapBatchFailed.title': 'Some updates failed',
  'notify.swapBatchFailed.description': 'Failed to update {failed} of {total} components.',
  'notify.rollbackBatchFailed.title': 'Some restores failed',
  'notify.rollbackBatchFailed.description': 'Failed to restore {failed} of {total} components.',
  'notify.statusError': 'Error',
  'notify.statusWarning': 'Warning',

  // ── Games toolbar ──
  'games.favoritesToggle': 'Favorites',
  'games.favoritesToggleActive': 'Favorites (active)',
  'games.showHidden': 'Hidden games',
  'games.showHiddenActive': 'Hidden games (active)',

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

  // ── DLSS indicator context (toasts) ──
  'indicator.relaunchToToggle': 'Restart as administrator to toggle the DLSS indicator.',
  'indicator.changeFailed': 'Failed to toggle the DLSS indicator',

  // ── Libraries table ──
  'libraries.column.version': 'Version',
  'libraries.column.hash': 'Hash',
  'libraries.column.signed': 'Signed',
  'libraries.column.size': 'Size',
  'libraries.column.documents': 'Documents',
  'libraries.column.actions': 'Actions',
  'libraries.documents.openForVersion': 'Open legal documents for {name} {version}',
  'libraries.documents.title': 'Legal documents',
  'libraries.documents.description': 'Applies to {name} {version}.',
  'libraries.documents.formatPdf': 'PDF',
  'libraries.documents.formatText': 'Text',
  'libraries.documents.open': 'Open',
  'libraries.documents.openFailed': 'Could not open the document',
  'libraries.unsigned': 'Unsigned',
  'libraries.invalidDate': 'Invalid date',
  'libraries.empty.loading': 'Loading…',
  'libraries.empty.unavailable': 'Unable to load libraries',
  'libraries.empty.none': 'No libraries found',
  'libraries.error.loadFailed': 'Failed to load libraries',
  'libraries.error.refreshFailed': 'Failed to refresh manifest',
  'libraries.error.downloadFailed': 'Download failed',
  'libraries.error.deleteFailed': 'Delete failed',
  'libraries.error.downloadedRefreshFailed': 'Library downloaded, but status refresh failed',
  'libraries.error.deletedRefreshFailed': 'Library deleted, but status refresh failed',

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
  'user_message.response_serialization_failed': 'Failed to process the request.',
  'user_message.plan_changed_rebuild': 'The task is outdated. Please try again.',
  'user_message.game_not_in_catalog': 'Game is not supported.',
  'user_message.operation_not_found': 'Action not found.',
  'user_message.artifact_not_found': 'Item not found.',
  'user_message.component_not_found': 'Component not found.',
  'user_message.invalid_operation_state': 'This action is currently unavailable.',
  'user_message.operation_could_not_complete': 'Failed to complete the action.',
  'user_message.command_task_failed': 'Failed to execute the command.',
  'user_message.storage_failed': 'The app could not read or write its catalog.',
  'user_message.provider_failed': 'A data source could not be read.',
  'user_message.detection_failed': 'The app could not analyze the game files.',
  'user_message.steamgriddb_api_key_missing':
    'Please provide a SteamGridDB API key in the settings.',
  'user_message.unsupported_cover_image_type': 'Unsupported image format.',
  'user_message.cover_download_failed': 'Failed to download the cover.',
  'user_message.cover_artwork_not_found': 'No cover found for this game.',
  'user_message.cover_file_system_error': 'Failed to save the cover to disk.',
  'user_message.stale_replacement_source':
    'This update could not be applied because the source file was replaced or modified outside RenderPilot. Please select the version again — a download may be needed.',
  'user_message.nvapi_requires_administrator':
    'Administrator rights are required to change this setting.',

  // ── Backend suggested actions ──
  'suggested_action.refresh_games': 'Refresh the games list and try again.',
  'suggested_action.reload_game_details': 'Refresh the game details and try again.',
  'suggested_action.refresh_candidates': 'Refresh the list and try again.',
  'suggested_action.rebuild_plan_or_reload_operations': 'Refresh the view and try again.',
  'suggested_action.retry_after_required_data': 'Please wait and try again later.',
  'suggested_action.inspect_logs': 'If the problem persists, try restarting the app.',
  'suggested_action.retry_or_restart': 'If the problem persists, try restarting the app.',
  'suggested_action.rebuild_operation_plan': 'Please restart the action.',
  'suggested_action.refresh_or_scan_game_folder': 'Refresh the list or scan the folder again.',
  'suggested_action.relaunch_as_administrator': 'Restart the app as administrator and try again.',
  // ── Game details: RenoDX ──
  'gameDetails.renodx.title': 'RenoDX HDR',
  'gameDetails.renodx.description':
    'Add HDR and tone-mapping to this game via the RenoDX ReShade add-on.',
  'gameDetails.renodx.loading': 'Checking availability…',
  'gameDetails.renodx.installError': 'RenoDX installation failed',
  'gameDetails.renodx.uninstallError': 'RenoDX removal failed',
  'gameDetails.renodx.switchError': 'ReShade channel switch failed',
  'gameDetails.renodx.unsupported': 'No RenoDX profile is available for this game.',
  'gameDetails.renodx.incompatible': 'RenoDX cannot be installed: {reason}.',
  'gameDetails.renodx.status.label': 'Status',
  'gameDetails.renodx.statusInstalled': 'Installed',
  'gameDetails.renodx.actionInstall': 'Install',
  'gameDetails.renodx.actionUninstall': 'Remove RenoDX',
  'gameDetails.renodx.actionRepair': 'Repair',
  'gameDetails.renodx.uninstallConfirmTitle': 'Remove RenoDX from this game?',
  'gameDetails.renodx.uninstallConfirmBody':
    'This removes the RenoDX add-on and restores only ReShade files that were changed during RenoDX setup.',
  'gameDetails.renodx.uninstallConfirmAction': 'Remove',
  'gameDetails.renodx.installing': 'Installing…',
  'gameDetails.renodx.confirmTitle': 'Install RenoDX despite anti-cheat risk?',
  'gameDetails.renodx.cancel': 'Cancel',
  // ── Game details: RenoDX shared Vulkan layer ──
  'gameDetails.renodx.vulkanLayer.title': 'Shared Vulkan layer',
  'gameDetails.renodx.vulkanLayer.removeConfirmTitle': 'Remove the shared Vulkan layer?',
  'gameDetails.renodx.vulkanLayer.removeConfirmBody':
    'Removing the shared ReShade Vulkan layer affects all Vulkan RenoDX games. Continue?',
  'gameDetails.renodx.vulkanLayer.openSettings': 'Open RenoDX settings',
  'gameDetails.renodx.vulkanLayer.removeError': "Couldn't remove the shared ReShade Vulkan layer.",
  'gameDetails.renodx.vulkanLayer.externalReadOnly':
    'Detected existing Vulkan layer; read-only in this version',
  'gameDetails.renodx.vulkanLayer.state.not_installed': 'Not installed',
  'gameDetails.renodx.vulkanLayer.state.installed': 'Installed',
  'gameDetails.renodx.vulkanLayer.state.installed_disabled': 'Disabled in registry',
  'gameDetails.renodx.vulkanLayer.state.external_read_only': 'Read-only',
  'gameDetails.renodx.vulkanLayer.state.conflict': 'Conflict',
  'gameDetails.renodx.vulkanLayer.state.needs_repair': 'Needs repair',
  'gameDetails.renodx.vulkanLayer.state.unsupported': 'Unsupported',
  'gameDetails.renodx.vulkanLayer.action.install': 'Install',
  'gameDetails.renodx.vulkanLayer.action.update': 'Update',
  'gameDetails.renodx.vulkanLayer.action.switch_channel': 'Switch channel',
  'gameDetails.renodx.vulkanLayer.action.repair': 'Repair layer',
  'gameDetails.renodx.vulkanLayer.action.remove': 'Remove',
  'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected':
    'An existing Vulkan layer was detected.',
  'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest':
    'Multiple ReShade layer manifests are registered.',
  'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility':
    'Loader visibility is ambiguous.',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll': 'The layer DLL is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll':
    'The layer DLL could not be read (permission denied or locked).',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest': 'The layer manifest is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing':
    'Layer files exist, but Vulkan loader registration is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled':
    'The loader registry entry is disabled.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture':
    'The layer architecture is unsupported.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated':
    'The layer is registered under HKCU and may not load for elevated games.',
  'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed':
    'A layer manifest could not be parsed.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable':
    'The required registry scope cannot be written.',
  'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied':
    'The operating system denied a required operation.',
  'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed':
    'Backend validation failed; the layer needs review.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch':
    'The layer DLL hash does not match the expected version.',
  'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback':
    'The layer DLL is missing; using advisory database record.',
  // ── Game details: RenoDX incompatibility reasons ──
  'gameDetails.renodx.reason.api_unsupported': 'unsupported graphics API',
  'gameDetails.renodx.reason.api_not_allowed': 'graphics API not allowed for this game',
  'gameDetails.renodx.reason.arch_unknown': 'unknown executable architecture',
  // ── Game details: RenoDX tab / states / version picker ──
  'gameDetails.otherTab': 'Other',
  'gameDetails.renodx.unavailable': 'RenoDX is unavailable right now.',
  // ── Game details: RenoDX generic (engine-fallback) labels ──
  'renodx.generic.universal': 'Universal RenoDX',
  'renodx.generic.unity': 'Universal RenoDX (Unity)',
  'gameDetails.renodx.generic.profileTooltip': 'A shared engine profile is being used.',
  'renodx.phase.finalizing': 'Finalizing…',
  'luma.phase.finalizing': 'Finalizing…',
  // ── Game details: RenoDX confidence / external / native-HDR / update ──
  'gameDetails.renodx.confidenceLabel': 'RenoDX compatibility',
  'gameDetails.renodx.confidenceVerified': 'Works',
  'gameDetails.renodx.confidenceExperimental': 'In progress',
  'gameDetails.renodx.confidenceUntested': 'Unverified',
  'gameDetails.renodx.external':
    'This RenoDX add-on is distributed externally and must be downloaded manually.',
  'gameDetails.renodx.actionOpenExternal': 'Open download page',
  'gameDetails.renodx.external.installFromFile': 'Install from file',
  'gameDetails.renodx.external.dropHint':
    'Download the add-on, then drop it here or pick the file.',
  'gameDetails.renodx.external.invalidFile':
    'That file is not a RenoDX add-on (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.title': 'Manual install',
  'gameDetails.renodx.fileInstall.chooseFile': 'Choose add-on file…',
  'gameDetails.renodx.fileInstall.chooseAnother': 'Choose another file',
  'gameDetails.renodx.fileInstall.expected': 'Expected add-on: {name}',
  'gameDetails.renodx.fileInstall.confirm': 'Install {fileName}?',
  'gameDetails.renodx.fileInstall.errorExtension':
    'That file is not a RenoDX add-on (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.errorArch':
    'This add-on is {addon} but the game is {game}. Download the matching add-on.',
  'gameDetails.renodx.fileInstall.warnName':
    'This doesn’t look like the expected add-on ({expected}). Install only if you’re sure.',
  'gameDetails.renodx.nativeHdr': 'This game already supports native HDR — RenoDX is not needed.',
  'gameDetails.renodx.blacklisted': 'RenoDX is not recommended for this game.',
  'gameDetails.renodx.updatesNotTracked': 'Updates not tracked',
  'gameDetails.renodx.channel.label': 'ReShade host channel',
  'gameDetails.renodx.channel.hostLabel': 'ReShade host',
  'gameDetails.renodx.channel.stable': 'Stable',
  'gameDetails.renodx.channel.nightly': 'Nightly',
  'gameDetails.renodx.host.version': '{version}',
  'gameDetails.renodx.host.versionUnknown': 'Version unknown',
  'gameDetails.renodx.host.addons.none': 'add-ons not supported',
  'gameDetails.renodx.host.addons.unknown': 'add-on support unknown',
  'gameDetails.renodx.host.action.update_host': 'update available',
  'gameDetails.renodx.host.action.repair_host': 'Repair ReShade for RenoDX add-on support',
  'gameDetails.renodx.host.customBuild': 'Custom build (e.g. GShade) — you manage updates yourself',
  'gameDetails.renodx.host.conflictMultiple':
    'Multiple ReShade hosts found — active slot needs review',
  'gameDetails.renodx.host.conflictBlocksInstall':
    'An existing file occupies the ReShade slot this game uses, or ReShade is in another slot — resolve it before installing.',
  'gameDetails.renodx.actionUpdate': 'Update',
  'gameDetails.renodx.updating': 'Updating…',
  'gameDetails.renodx.updateError': 'RenoDX update failed',
  'gameDetails.renodx.actionInstallDlssFix': 'Install',
  'gameDetails.renodx.actionRemoveDlssFix': 'Remove',
  'gameDetails.renodx.dlssFixInstallError': 'DLSS-Fix installation failed',
  'gameDetails.renodx.dlssFixRemoveError': 'DLSS-Fix removal failed',
  'gameDetails.renodx.fresh.label': 'Version',
  'gameDetails.renodx.fresh.current': 'Latest',
  'gameDetails.renodx.fresh.available': 'Update available',
  'gameDetails.renodx.fresh.channelMismatch': 'Channel change available',
  'gameDetails.renodx.fresh.validationRequired': 'Validation required',
  'gameDetails.renodx.fresh.unknown': "Couldn't check",
  'gameDetails.renodx.fresh.checking': 'Checking…',
  'gameDetails.renodx.addonDated': 'Add-on dated {date}',
  'gameDetails.renodx.installedOn': 'Installed {date}',
  'gameDetails.renodx.lastChecked': 'Checked {time}',
  'gameDetails.renodx.lastCheckedNever': 'Not checked yet',
  'gameDetails.renodx.actionCheckUpdates': 'Check for updates',
  'gameDetails.renodx.component.reshade': 'ReShade host',
  'gameDetails.renodx.component.addon': 'RenoDX add-on',
  'gameDetails.renodx.component.addonDesc': 'The HDR add-on for this game',
  'gameDetails.renodx.component.addonDisabled': 'Installed, but disabled in ReShade.ini',
  'gameDetails.renodx.component.addonFileInstall':
    'Installed from a file — not tracked for updates',
  'gameDetails.renodx.component.dlssFix': 'DLSS-Fix',
  'gameDetails.renodx.component.dlssFixDesc': 'Fixes flickering with DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixOffer':
    'Available — prevents flickering with DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixHint':
    "A general ReShade fix, not RenoDX-specific. It makes ReShade draw on the game's native frames instead of Frame-Generation frames, and hides DLSS upscaling from ReShade when the game implements Streamline correctly.",
  'gameDetails.renodx.attribution': 'RenoDX by clshortfuse.',
  'gameDetails.renodx.attributionLink': 'View project',
  // ── Game details: shared add-on copy (RenoDX + Luma) ──
  'gameDetails.addon.riskSafe': 'No anti-cheat detected — safe to install.',
  'gameDetails.addon.riskWarn': 'Anti-cheat detected — installing may risk a ban.',
  'addon.risk.sp_safe':
    'No known anti-cheat signatures were found — installing {addonName} is likely safe, but not guaranteed.',
  'addon.risk.anticheat_detected':
    'Anti-cheat signatures were detected — installing {addonName} may risk a ban.',
  'gameDetails.addon.confirmAccept': 'Install anyway',
  'gameDetails.addon.confirmBody':
    'This game uses anti-cheat. The ReShade add-on may trigger it and get you banned. Continue at your own risk.',
  'gameDetails.addon.fullAddonWarning':
    'ReShade full add-on support can be unsafe for multiplayer or anti-cheat protected games.',
  'gameDetails.addon.blockedByOtherAddon.tracked':
    '{installedAddon} is installed for this game — uninstall it before installing {blockedAddon}.',
  'gameDetails.addon.blockedByOtherAddon.unmanaged':
    '{installedAddon} files were found on disk for this game — remove them before installing {blockedAddon}.',
  'addon.availability.loadFailed': 'Could not check',
  'addon.availability.retry': 'Retry',
  'addon.availability.checking': 'Checking…',
  // ── Game details: Luma ──
  'gameDetails.luma.title': 'Luma Framework',
  'gameDetails.luma.description': 'Luma features available for this game are listed below.',
  'gameDetails.luma.loading': 'Checking availability…',
  'gameDetails.luma.installError': 'Luma installation failed',
  'gameDetails.luma.uninstallError': 'Luma removal failed',
  'gameDetails.luma.updateError': 'Luma update failed',
  'gameDetails.luma.repairError': 'Luma repair failed',
  'gameDetails.luma.unsupported': 'No Luma profile is available for this game.',
  'gameDetails.luma.incompatible': 'Luma cannot be installed: {reason}.',
  'gameDetails.luma.blacklisted': 'Luma is not recommended for this game.',
  'gameDetails.luma.unavailable': 'Luma is unavailable right now.',
  'gameDetails.luma.unmanagedPresent':
    'An existing Luma install was found on disk with no tracked record. Remove it manually, then reinstall.',
  'gameDetails.luma.installTornWarning':
    'A previous install did not finish cleanly. Installing again will clean it up and repair it.',
  'gameDetails.luma.installTornWarningInstalled':
    'The last operation did not finish cleanly. Use Repair (or Update if shown) to finish reconciling the install.',
  'gameDetails.luma.status.label': 'Status',
  'gameDetails.luma.statusInstalled': 'Installed',
  'gameDetails.luma.actionInstall': 'Install',
  'gameDetails.luma.installing': 'Installing…',
  'gameDetails.luma.actionUninstall': 'Remove Luma',
  'gameDetails.luma.actionRepair': 'Repair',
  'gameDetails.luma.actionUpdate': 'Update',
  'gameDetails.luma.updating': 'Updating…',
  'gameDetails.luma.actionCheckUpdates': 'Check for updates',
  'gameDetails.luma.uninstallConfirmTitle': 'Remove Luma from this game?',
  'gameDetails.luma.uninstallConfirmBody':
    'This removes Luma. If Luma owns the DLSS DLL, its Library Swap is rolled back and the exact pre-Luma baseline is restored. Reused DLLs and independent swaps stay unchanged.',
  'gameDetails.luma.uninstallConfirmAction': 'Remove',
  'gameDetails.luma.confirmTitle': 'Install Luma despite anti-cheat risk?',
  'gameDetails.luma.vcredistWarning':
    'A recent Visual C++ Redistributable may be missing on this system. If Luma fails to load, install the redistributable.',
  'gameDetails.luma.vcredistLink': 'Download the redistributable',
  'gameDetails.luma.dgvoodoo.managed':
    'RenderPilot will install and configure dgVoodoo2 {version} for this Luma profile.',
  // ── Game details: Luma confidence ──
  'gameDetails.luma.confidenceLabel': 'Luma compatibility',
  'gameDetails.luma.confidenceVerified': 'Works',
  'gameDetails.luma.confidenceExperimental': 'In progress',
  'gameDetails.luma.confidenceUntested': 'Unverified',
  'gameDetails.luma.generic.engineUnreal': 'Unreal Engine',
  'gameDetails.luma.generic.engineUnity': 'Unity',
  'gameDetails.luma.generic.profileTooltip': 'A shared engine profile is being used.',
  'gameDetails.luma.features.title': 'Features',
  'gameDetails.luma.features.dlssFsr': 'DLSS / FSR',
  'gameDetails.luma.features.hdr': 'HDR',
  'gameDetails.luma.features.supported': 'Supported',
  'gameDetails.luma.features.unsupported': 'Not supported',
  'gameDetails.luma.features.experimental': 'Experimental',
  'gameDetails.luma.features.unknown': 'Unknown',
  'gameDetails.luma.guidance.gameSetting': 'In-game setting',
  'gameDetails.luma.guidance.engineIni': 'Manual INI change',
  'gameDetails.luma.guidance.launchArgument': 'Launch argument',
  'gameDetails.luma.guidance.warning': 'Important',
  'gameDetails.luma.guidance.compatibility': 'Compatibility note',
  'gameDetails.luma.guidance.externalTool': 'Third-party tool',
  'gameDetails.luma.guidance.copy': 'Copy',
  'gameDetails.luma.guidance.copied': 'Copied',
  'gameDetails.luma.guidance.copyFailed': 'Could not copy',
  // ── Game details: Luma incompatibility reasons ──
  'gameDetails.luma.reason.api_unsupported': 'unsupported graphics API',
  'gameDetails.luma.reason.api_not_allowed': 'graphics API not allowed for this game',
  'gameDetails.luma.reason.arch_unknown': 'unknown executable architecture',
  'gameDetails.luma.reason.arch_mismatch': 'executable architecture does not match this add-on',
  // ── Game details: Luma ReShade host ──
  'gameDetails.luma.channel.stable': 'Stable',
  'gameDetails.luma.channel.nightly': 'Nightly',
  'gameDetails.luma.host.version': '{version}',
  'gameDetails.luma.host.versionUnknown': 'Version unknown',
  'gameDetails.luma.host.addons.none': 'add-ons not supported',
  'gameDetails.luma.host.addons.unknown': 'add-on support unknown',
  'gameDetails.luma.host.action.update_host': 'update available',
  'gameDetails.luma.host.action.repair_host': 'Repair ReShade for Luma add-on support',
  'gameDetails.luma.host.customBuild': 'Custom build (e.g. GShade) — you manage updates yourself',
  'gameDetails.luma.host.conflictMultiple':
    'Multiple ReShade hosts found — active slot needs review',
  'gameDetails.luma.host.conflictBlocksInstall':
    'An existing file occupies the ReShade slot this game uses, or ReShade is in another slot — resolve it before installing.',
  // ── Game details: Luma freshness / timestamps ──
  'gameDetails.luma.fresh.label': 'Version',
  'gameDetails.luma.fresh.current': 'Latest',
  'gameDetails.luma.fresh.available': 'Update available',
  'gameDetails.luma.fresh.channelMismatch': 'Channel change available',
  'gameDetails.luma.fresh.validationRequired': 'Validation required',
  'gameDetails.luma.fresh.unknown': "Couldn't check",
  'gameDetails.luma.fresh.checking': 'Checking…',
  'gameDetails.luma.updatesNotTracked': 'Updates not tracked',
  'gameDetails.luma.addonDated': 'Add-on dated {date}',
  'gameDetails.luma.installedOn': 'Installed {date}',
  'gameDetails.luma.lastChecked': 'Checked {time}',
  'gameDetails.luma.lastCheckedNever': 'Not checked yet',
  // ── Game details: Luma components ──
  'gameDetails.luma.component.reshade': 'ReShade host',
  'gameDetails.luma.component.addon': 'Luma add-on',
  'gameDetails.luma.component.addonDesc': 'Luma features for this game',
  'gameDetails.luma.component.dgvoodoo': 'dgVoodoo2 wrapper',
  'gameDetails.luma.component.dgvoodooDesc': 'Managed D3D9 bridge, version {version}',
  // ── Game details: Luma launch arguments ──
  'gameDetails.luma.launchArgs.instructions.steam':
    'If you start the game through Steam, add them there: right-click the game → Properties → General → Launch Options.',
  'gameDetails.luma.launchArgs.instructions.gog':
    'If you start the game through GOG Galaxy, add them there: game settings → Manage installation → Configure.',
  'gameDetails.luma.launchArgs.instructions.epic':
    'If you start the game through the Epic Games Launcher, add them there: right-click the game → Manage → Additional Command Line Arguments.',
  'gameDetails.luma.launchArgs.instructions.ea':
    'If you start the game through the EA app, add them there: select the game → Manage → View properties → Advanced launch options.',
  'gameDetails.luma.launchArgs.instructions.ubisoft':
    'If you start the game through Ubisoft Connect, add them there: select the game → Properties → Add launch arguments.',
  'gameDetails.luma.launchArgs.instructions.other':
    'Use the launch method that actually starts the game. Add the arguments to its launcher, shortcut target, batch file, or another loader.',
  'gameDetails.luma.launchArgs.title': 'Launch arguments required',
  'gameDetails.luma.launchArgs.dx11Title': 'This Luma profile requires DirectX 11',
  'gameDetails.luma.launchArgs.copyStep': 'Copy the required launch arguments:',
  'gameDetails.luma.launchArgs.copy': 'Copy arguments',
  'gameDetails.luma.launchArgs.copied': 'Copied',
  'gameDetails.luma.launchArgs.copyFailed': 'Could not copy the launch arguments',
  // ── Game details: Luma attribution ──
  'gameDetails.luma.attribution': 'Luma Framework by Filoppi.',
  'gameDetails.luma.attributionLink': 'View project',
} satisfies MessageDictionary;

export type MessageKey = keyof typeof en;
