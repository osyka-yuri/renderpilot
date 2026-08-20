import type { EnglishCatalog } from './en';
import { defineLocalizedCatalog } from './contract';
import { plural } from './model';

export const de = defineLocalizedCatalog<'de', EnglishCatalog>()({
  'nav.games': 'Spiele',
  'nav.libraries': 'Bibliotheken',
  'nav.settings': 'Einstellungen',
  'nav.operations': 'Journal',
  'nav.gameFallback': 'Spiel',
  'nav.donate': 'Spenden',
  'shell.refresh': 'Aktualisieren',
  'shell.updateAvailable': 'Update verfügbar',
  'nav.skipToContent': 'Zum Inhalt springen',
  'nav.primaryLabel': 'Hauptnavigation',
  'nav.breadcrumbLabel': 'Brotkrümelnavigation',
  'shell.sidebar.toggle': 'Seitenleiste umschalten',
  'shell.sidebar.title': 'Navigation',
  'shell.sidebar.description': 'Hauptnavigation der Anwendung.',
  'shell.notifications.regionLabel': 'Benachrichtigungen',
  'shell.notifications.close': 'Benachrichtigung schließen',
  'shell.pageTitle': '{page} — RenderPilot',

  'settings.appearance.title': 'Erscheinungsbild',
  'settings.appearance.description': 'Passen Sie das Aussehen der Anwendung und die Sprache an.',
  'settings.appearance.theme.title': 'Design',
  'settings.appearance.theme.description': 'Wählen Sie ein Farbdesign für die Anwendung.',
  'settings.appearance.theme.triggerLabel': 'Design',
  'settings.appearance.language.title': 'Sprache',
  'settings.appearance.language.description': 'Wählen Sie die Sprache der Benutzeroberfläche.',
  'settings.appearance.language.triggerLabel': 'Sprache',
  'settings.appearance.language.placeholder': 'Sprache auswählen',

  'settings.theme.system': 'System',
  'settings.theme.dark': 'Dunkel',
  'settings.theme.light': 'Hell',

  'settings.language.system': 'Systemstandard',
  'settings.language.en': 'English',
  'settings.language.ru': 'Русский',
  'settings.language.es': 'Español',
  'settings.language.zhHans': '简体中文',
  'settings.language.zhHant': '繁體中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  'settings.tabs.general': 'Allgemein',
  'settings.tabs.renodx': 'RenoDX',
  'settings.tabs.catalog': 'Katalog',
  'settings.tabs.nvidia': 'NVIDIA',

  // ── Settings: NVIDIA ──
  'settings.nvidia.indicator.title': 'DLSS-Indikator',
  'settings.nvidia.indicator.description':
    'Zeigt ein Overlay mit der aktiven DLSS-Version und den Einstellungen während des Spiels.',
  'settings.nvidia.indicator.systemWide': 'Systemweit',
  'settings.nvidia.indicator.overlayTitle': 'Bildschirm-Overlay',
  'settings.nvidia.indicator.overlayDescription': 'Gilt für alle Spiele auf diesem PC.',
  'settings.nvidia.indicator.toggleLabel': 'DLSS-Indikator umschalten',
  'settings.nvidia.global.title': 'Globale DLSS-Einstellungen',
  'settings.nvidia.global.description':
    'Standardwerte für jedes Spiel ohne spielspezifische Überschreibung, über das NVIDIA-Basisprofil.',
  'settings.nvidia.global.systemWide': 'Systemweit',
  'settings.nvidia.global.familySr': 'DLSS Superhohe Auflösung',
  'settings.nvidia.global.familyFg': 'DLSS Frame-Erstellung',
  'settings.nvidia.global.familyRr': 'DLSS Strahlenrekonstruktion',
  'settings.nvidia.unsupported.title': 'Keine NVIDIA-GPU erkannt',
  'settings.nvidia.unsupported.description':
    'Diese Einstellungen erfordern eine unterstützte NVIDIA-Grafikkarte.',

  'game.card.action.details': 'Details',
  'game.card.action.detailsLabel': 'Details für {title} öffnen',
  'game.card.detectedLibraries': 'Erkannte Komponenten',
  'game.card.availableAddons': 'Verfügbare Add-ons',
  'game.card.badge.upToDate': 'Aktuell',
  'game.card.badge.updatesAvailable': 'Updates verfügbar',
  'game.card.badge.updatesAvailableCount': plural('count', {
    one: '1 Update verfügbar',
    other: '{count} Updates verfügbar',
  }),
  'game.card.status.favorite': 'Favorit',
  'game.card.status.hidden': 'Ausgeblendet',
  'game.card.menu.label': 'Optionen für {title}',
  'game.card.menu.favorite.add': 'Zu Favoriten hinzufügen',
  'game.card.menu.favorite.remove': 'Aus Favoriten entfernen',
  'game.card.menu.favorite.toggleHint': 'Favoritenstatus für dieses Spiel umschalten.',
  'game.card.menu.hidden.add': 'Spiel verstecken',
  'game.card.menu.hidden.remove': 'Spiel einblenden',
  'game.card.menu.hidden.toggleHint': 'Versteckt-Status für dieses Spiel umschalten.',
  'game.card.menu.removeFromCatalog': 'Aus Katalog entfernen',
  'game.card.menu.removeFromCatalogHint': 'Dieses manuell hinzugefügte Spiel vergessen.',
  'game.card.removeConfirm.title': '{title} aus dem Katalog entfernen?',
  'game.card.removeConfirm.description':
    'RenderPilot macht verwaltete Änderungen sicher rückgängig und entfernt anschließend die Karte samt Verlauf. Die Dateien des Spiels bleiben unverändert.',
  'game.card.removeConfirm.action': 'Aus Katalog entfernen',

  'game.cover.alt': 'Cover',
  'game.cover.altWithTitle': 'Cover: {title}',
  'game.cover.menu.fetch': 'Cover herunterladen',
  'game.cover.menu.fetching': 'Wird heruntergeladen…',
  'game.cover.menu.fetchHint': 'Nach einem Cover online suchen.',
  'game.cover.menu.pick': 'Bilddatei auswählen…',
  'game.cover.menu.pickHint': 'Wählen Sie ein lokales Bild als Cover aus.',
  'game.cover.menu.clear': 'Cover entfernen',
  'game.cover.menu.clearHint': 'Standard-Cover wiederherstellen.',

  'game.dashboard.summary': 'Dashboard',
  'game.dashboard.games': plural('count', { one: '{count} Spiel', other: '{count} Spiele' }),
  'game.dashboard.updates': plural('count', { one: '{count} Update', other: '{count} Updates' }),

  'error.boundary.title': 'Etwas ist schiefgelaufen',
  'error.boundary.description':
    'Auf diesem Bildschirm ist ein unerwarteter Fehler aufgetreten. Versuchen Sie es erneut oder wechseln Sie in einen anderen Bereich.',
  'error.boundary.reset': 'Erneut versuchen',
  'error.desktopTransportFailed':
    'Der Desktop-Dienst hat eine ungültige Antwort zurückgegeben. Versuchen Sie die Aktion erneut.',
  'error.unexpectedClient':
    'Ein unerwarteter Fehler ist aufgetreten. Versuchen Sie die Aktion erneut.',
  'error.localeLoadFailed':
    'Die ausgewählte Sprache konnte nicht geladen werden. Die vorherige Sprache bleibt aktiv.',
  'error.recoveryBundlePath': 'Wiederherstellungspaket: {path}',
  'pageLoad.loading': 'Seite wird geladen…',
  'pageLoad.error.title': 'Diese Seite konnte nicht geöffnet werden',
  'pageLoad.error.description':
    'Die Seite konnte nicht geladen werden. Versuchen Sie es erneut oder kehren Sie zu den Spielen zurück.',
  'pageLoad.error.retry': 'Erneut versuchen',
  'pageLoad.error.backToGames': 'Zurück zu den Spielen',

  'games.addGame': 'Spiel hinzufügen',
  'games.addingGame': 'Spiel wird hinzugefügt…',
  'games.chooseInstallFolder': 'Installationsordner des Spiels auswählen',
  'addGame.title': 'Spiel hinzufügen',
  'addGame.cannotAddTitle': 'Das Spiel konnte nicht hinzugefügt werden',
  'addGame.installRoot': 'Installationsordner',
  'addGame.reviewTitle': 'Spielinstallation prüfen',
  'addGame.reviewDescription':
    'Bestätigen Sie den Installationsordner, bevor ein Spiel hinzugefügt wird.',
  'addGame.selectedFolder': 'Ausgewählter Ordner',
  'addGame.recommendedFolder': 'Empfohlener Installationsordner',
  'addGame.existingRoot': 'Aktueller Spielordner',
  'addGame.chooseExecutable': 'Ausführbare Spieldatei',
  'addGame.chooseExecutablePlaceholder': 'Ausführbare Datei auswählen',
  'addGame.chooseAnother': 'Anderen auswählen',
  'addGame.add': 'Spiel hinzufügen',
  'addGame.addSelected': 'Ausgewählten Ordner hinzufügen',
  'addGame.correctRoot': 'Spielpfad korrigieren',
  'addGame.addRecommended': 'Empfohlenen Ordner hinzufügen',
  'addGame.replaceRootTitle': 'Spielpfad korrigieren',
  'addGame.replaceRootDescription':
    'RenderPilot verwendet den ausgewählten Ordner anstelle des aktuellen. Die Spieldateien bleiben unverändert.',
  'addGame.replaceExistingRoot': 'Spielpfad korrigieren',
  'addGame.rootCorrection.rollbackTitle':
    'Aktive Komponentenänderungen müssen zuerst rückgängig gemacht werden',
  'addGame.rootCorrection.rollbackDescription': plural('count', {
    one: 'RenderPilot muss die aktive Änderung an einer Komponente rückgängig machen, bevor der Kartenordner ersetzt wird.',
    other:
      'RenderPilot muss die aktiven Änderungen an {count} Komponenten rückgängig machen, bevor der Kartenordner ersetzt wird.',
  }),
  'addGame.rootCorrection.rollbackAndReplace': 'Änderungen rückgängig machen und Ordner ersetzen',
  'addGame.rootCorrection.rollbackFailed':
    'Die Komponentenänderungen konnten nicht vollständig rückgängig gemacht werden. Der vorhandene Spielordner wurde nicht geändert.',
  'addGame.rootCorrection.blocker.pendingRecovery':
    'Ein unterbrochener Dateivorgang muss noch wiederhergestellt werden.',
  'addGame.rootCorrection.blocker.installedAddon':
    'Ein installiertes Add-on gehört zu Dateien außerhalb des ausgewählten Spielordners.',
  'addGame.rootCorrection.blocker.nvapi':
    'Aktive NVIDIA-Profileinstellungen gehören zu Programmdateien außerhalb des ausgewählten Ordners.',
  'addGame.rootCorrection.blocker.orphanedComponentBaseline':
    'Für einen gespeicherten Rollback-Zustand gibt es keine passende Komponente mehr.',
  'addGame.rescan': 'Spiel erneut scannen',
  'addGame.catalogBusy':
    'Ein anderer Katalogvorgang wird noch ausgeführt. Schließen Sie ihn ab und versuchen Sie es erneut.',
  'addGame.warning.legacyCardsConsolidated': plural('count', {
    one: 'Eine nachweislich falsche ältere Spielkarte wurde zusammengeführt.',
    other: '{count} nachweislich falsche ältere Spielkarten wurden zusammengeführt.',
  }),
  'addGame.warning.legacyCardsRetained': plural('count', {
    one: 'Eine ältere Spielkarte wurde beibehalten, da die Hinweise auf eine eigenständige Installation nicht eindeutig waren.',
    other:
      '{count} ältere Spielkarten wurden beibehalten, da die Hinweise auf eigenständige Installationen nicht eindeutig waren.',
  }),
  'addGame.warning.recoveryBundleCreated':
    'In Konflikt stehende ältere Daten wurden im Wiederherstellungspaket {path} gesichert.',
  'addGame.warning.rootCorrectionHistoryArchived':
    'Katalogverlauf außerhalb des korrigierten Spielordners wurde im Wiederherstellungspaket {path} gesichert.',
  'addGame.warning.recoveryBundleFallback': 'Wiederherstellungspaket: {path}',
  'addGame.warning.unsupportedPlatform':
    'Die Prüfung von Spielinstallationen wird nur unter Windows unterstützt.',
  'addGame.warning.probeIncomplete':
    'Einige Ordner konnten nicht geprüft werden. Die Empfehlung ist daher weniger zuverlässig.',
  'addGame.warning.parentProbeIncomplete':
    'Der empfohlene übergeordnete Ordner konnte nicht vollständig geprüft werden. Prüfen Sie ihn vor dem Hinzufügen.',
  'addGame.unavailable.multipleInstalls':
    'Der ausgewählte Ordner scheint eine gemeinsame Bibliothek mit mehreren Spielen zu sein. Wählen Sie den Ordner eines einzelnen Spiels aus.',
  'addGame.unavailable.containsProvenInstall':
    'Im ausgewählten Ordner befindet sich eine bereits erkannte Spielinstallation. Wählen Sie den genauen Ordner dieses Spiels statt des gemeinsamen übergeordneten Ordners.',
  'addGame.unavailable.containsMultipleCatalogInstalls':
    'Im ausgewählten Ordner befinden sich mehrere bereits erkannte Spiele. Wählen Sie den Ordner eines einzelnen Spiels aus.',
  'addGame.unavailable.insideExistingInstall':
    'Der ausgewählte Ordner liegt innerhalb eines bereits hinzugefügten Spiels. Verwenden Sie dessen Installationsstammordner.',
  'addGame.unavailable.noReadableExecutable':
    'Im ausgewählten Ordner wurde keine lesbare ausführbare Spieldatei gefunden. Wählen Sie den Installationsordner mit der ausführbaren Spieldatei.',
  'addGame.unavailable.rootCorrectionBlocked':
    'Der vorhandene Installationsstammordner kann nicht sicher geändert werden, solange verwaltete Zustände vorhanden sind. Beheben Sie zuerst die aufgeführten Blockierungen.',
  'addGame.warning.insideExistingInstall':
    'Dieser Ordner gehört zu einem vorhandenen Spiel. Verwenden Sie dessen Installationsordner.',
  'addGame.warning.narrowsExistingInstall':
    'Der vorhandene manuelle Stammordner scheint mehrere Spielordner zu enthalten. Bei Bestätigung bleibt dieselbe Karte erhalten, ihr Stammordner wird jedoch auf den ausgewählten Ordner korrigiert.',
  'addGame.warning.multipleProvenInstalls':
    'Dieser Ordner enthält mehrere bestätigte Spielinstallationen.',
  'addGame.warning.containsProvenInstall':
    'Dieser Ordner enthält eine bestätigte Spielinstallation. Verwenden Sie deren genauen Installationsordner.',
  'addGame.warning.multipleInstallsSuspected':
    'Ausführbare Dateien in getrennten Unterordnern können zu verschiedenen Spielen gehören. Bei Bestätigung wird der Ordner dennoch als ein Spiel behandelt.',
  'addGame.warning.explicitExecutableRequired':
    'Alle gültigen ausführbaren Dateien sehen wie Launcher oder Hilfsprogramme aus. Wählen Sie eine Datei ausdrücklich aus.',
  'addGame.warning.noReadableExecutable':
    'Dieser Ordner kann nicht separat hinzugefügt werden, da er keine lesbare Spiel-Programmdatei enthält.',
  'addGame.warning.filesystemProbeError':
    'Ein Teil der Installation konnte nicht geprüft werden. Überprüfen Sie die Dateizugriffsrechte.',
  'addGame.warning.unknown':
    'Bei der Spielprüfung ist eine Warnung aufgetreten, die diese RenderPilot-Version nicht anzeigen kann.',
  'games.libraryActions': 'Aktionen',
  'games.search': 'Spiele suchen',
  'games.openFilters': 'Filter',
  'games.openFiltersActive': 'Filter (aktiv)',
  'games.loading': 'Laden…',
  'games.empty.title': 'Keine Spiele gefunden',
  'games.empty.description': 'Fügen Sie ein Spiel hinzu, damit es im Dashboard angezeigt wird.',
  'games.filterEmpty.title': 'Keine Treffer gefunden',
  'games.filterEmpty.description': 'Versuchen Sie, Ihre Suche oder Filter zu ändern.',
  'games.filterEmpty.reset': 'Filter zurücksetzen',

  'settings.catalog.title': 'Cover-Quellen',
  'settings.catalog.description':
    'Wählen Sie Online-Quellen zum Herunterladen von Spiel-Covern aus.',
  'settings.catalog.steamKey.inputLabel': 'SteamGridDB API-Schlüssel',
  'settings.catalog.steamKey.placeholder': 'API-Schlüssel',
  'settings.catalog.steamKey.loading': 'Laden…',
  'settings.catalog.steamKey.save': 'Speichern',
  'settings.catalog.steamKey.saved': 'Gespeichert',
  'settings.catalog.steamKey.cleared': 'Gelöscht',
  'settings.catalog.steamKey.readError': 'Einstellungen konnten nicht gelesen werden.',
  'settings.catalog.steamKey.saveError': 'Einstellungen konnten nicht gespeichert werden.',
  'settings.catalog.steamKey.show': 'API-Schlüssel anzeigen',
  'settings.catalog.steamKey.hide': 'API-Schlüssel ausblenden',
  'settings.catalog.steamKey.getKey': 'API-Schlüssel erhalten',

  'settings.renodx.vulkan.description':
    'Verwalte den gemeinsamen ReShade-Vulkan-Layer für Vulkan-RenoDX-Spiele.',
  'settings.renodx.vulkan.channel': 'Vulkan-Layer-Kanal',
  'settings.renodx.vulkan.channelDescription':
    'Wähle den ReShade-Kanal für den gemeinsamen Vulkan-Layer.',
  'settings.renodx.vulkan.loadError': 'Vulkan-Layer-Status konnte nicht geladen werden.',
  'settings.renodx.vulkan.saveError': 'Vulkan-Layer-Kanal konnte nicht gespeichert werden.',
  'settings.renodx.vulkan.applyError': 'Vulkan-Layer konnte nicht angewendet werden.',

  'common.unknown': 'Unbekannt',
  'common.downloadProgress': 'Download-Fortschritt',
  'common.close': 'Schließen',

  'gameDetails.noGameSelected.title': 'Kein Spiel ausgewählt',
  'gameDetails.noGameSelected.description':
    'Wählen Sie ein Spiel aus dem Dashboard, um die Details anzuzeigen.',

  'gameDetails.version.noReplacements': 'Keine alternativen Versionen',
  'gameDetails.version.restoreOriginal': 'Original {fileName} wiederherstellen',
  'gameDetails.version.fileCount': plural('count', { one: '1 Datei', other: '{count} Dateien' }),

  'gameDetails.vendor.description': 'Ändern Sie die Komponentenversion.',

  'gameDetails.dlss.description':
    'Ändern Sie die DLSS-Version oder überschreiben Sie die Einstellungen.',
  'gameDetails.dlss.descriptionSwapOnly': 'Ändern Sie die DLSS-Version.',
  'gameDetails.dlss.libraryFileLabel': 'Dateiversion',
  'gameDetails.dlss.driverOverridesLabel': 'NVIDIA-Profil-Überschreibungen',

  'gameDetails.streamline.description': 'Streamline-Plugins verwalten.',
  'gameDetails.streamline.versionTitle': 'Globale Streamline-Version',
  'gameDetails.streamline.versionDescription': 'Wendet dieselbe Version auf alle Plugins an.',
  'gameDetails.streamline.noOtherVersions': 'Keine anderen Versionen',
  'gameDetails.streamline.mixed': 'Gemischte Versionen',
  'gameDetails.streamline.mixedRange': 'Gemischte Versionen (v{min} – v{max})',
  'gameDetails.streamline.updatesSummary': '{updates} Updates · {missing} fehlen',
  'gameDetails.streamline.restoreAllLabel': 'Alle Plugins auf Original wiederherstellen',
  'gameDetails.streamline.restoreAllTooltip': 'Alle auf Original wiederherstellen',
  'gameDetails.updateAll.action': 'Alle aktualisieren',
  'gameDetails.updateAll.actionCount': 'Alle aktualisieren ({count})',
  'gameDetails.updateAll.upToDate': 'Alle stabilen Versionen sind aktuell',
  'gameDetails.updateAll.partialFailure':
    'Einige Updates sind fehlgeschlagen ({count}). Details prüfen und erneut versuchen.',
  'gameDetails.updateAll.tooltip': plural('count', {
    one: '1 Komponente auf die neueste stabile Version aktualisieren',
    other: '{count} Komponenten jeweils auf die neueste stabile Version aktualisieren',
  }),
  'gameDetails.executable.title': 'Spiel-Programmdatei',
  'gameDetails.executable.groupLabel': 'Verfügbare ausführbare Spieldateien',
  'gameDetails.developerMode.requiredTitle': 'Windows-Entwicklermodus ist deaktiviert',
  'gameDetails.developerMode.requiredDescription':
    'Microsoft D3D12 Agility Preview benötigt diese Windows-Einstellung.',
  'gameDetails.developerMode.checkTitle': 'Entwicklermodus konnte nicht geprüft werden',
  'gameDetails.developerMode.checkDescription':
    'RenderPilot konnte den aktuellen Status des Windows-Entwicklermodus nicht ermitteln.',
  'gameDetails.developerMode.checkUnavailable':
    'Vor dem Fortfahren ist eine erfolgreiche Prüfung erforderlich.',
  'gameDetails.developerMode.enableGuidance':
    'Der Entwicklermodus kann unter „Für Entwickler“ in den Windows-Einstellungen aktiviert werden.',
  'gameDetails.developerMode.previewGuidance':
    'Die Microsoft-Dokumentation erklärt, wie der Entwicklermodus in Windows aktiviert wird.',
  'gameDetails.developerMode.restartInfo':
    'In einigen Fällen wird diese Einstellung erst nach einem Neustart von Windows wirksam.',
  'gameDetails.developerMode.stillDisabled':
    'Der Entwicklermodus ist weiterhin deaktiviert. Wenn er erst kürzlich aktiviert wurde, muss Windows möglicherweise neu gestartet werden, bevor die Änderung wirksam wird.',
  'gameDetails.developerMode.settingsOpenFailed':
    'Die Windows-Einstellungen konnten nicht geöffnet werden. Öffnen Sie „Für Entwickler“ manuell.',
  'gameDetails.developerMode.documentationOpenFailed':
    'Die Microsoft-Dokumentation konnte nicht geöffnet werden.',
  'gameDetails.developerMode.openSettings': 'Einstellungen öffnen',
  'gameDetails.developerMode.openDocumentation': 'Dokumentation öffnen',
  'gameDetails.developerMode.checkStatus': 'Status prüfen',
  'gameDetails.developerMode.retryCheck': 'Prüfung wiederholen',
  'gameDetails.developerMode.checkingStatus': 'Wird geprüft…',
  'gameDetails.d3d12.status.original': 'Originale EXE',
  'gameDetails.d3d12.status.patched': 'EXE gepatcht: {from} → {to}',
  'gameDetails.d3d12.status.repair': 'Reparatur erforderlich',
  'gameDetails.d3d12.repairGuidance':
    'Spieldateien prüfen und erneut scannen. RenderPilot überschreibt diese EXE nicht.',
  'gameDetails.d3d12.action.patch': 'EXE patchen: {from} → {to}',
  'gameDetails.d3d12.action.restore': 'EXE wiederherstellen: {from} → {to}',
  'gameDetails.d3d12.action.repair': 'EXE muss zuerst repariert werden',
  'gameDetails.d3d12.action.blocked':
    'Diese D3D12-Version kann im aktuellen Zustand nicht angewendet werden.',
  'gameDetails.d3d12.action.planPatch': 'Patch wird angewendet: SDK {from} → {to}',
  'gameDetails.d3d12.action.planRestore':
    'Die Original-EXE wird wiederhergestellt: SDK {from} → {to}',
  'gameDetails.d3d12.select.compatible': 'Mit der aktuellen EXE kompatibel',
  'gameDetails.d3d12.select.changesExecutable': 'EXE-Änderung erforderlich',
  'gameDetails.d3d12.select.unavailable': 'Nicht verfügbar',
  'gameDetails.d3d12.confirm.title': 'EXE-Änderung bestätigen',
  'gameDetails.d3d12.confirm.description':
    'RenderPilot ändert den D3D12SDKVersion-Export der Spiel-EXE.',
  'gameDetails.d3d12.confirm.updateAllDescription':
    'Für diese Updates müssen die aufgeführten Spiel-EXEs ihre D3D12-SDK-Linie wechseln. Vor der Bestätigung wird nichts heruntergeladen oder geändert.',
  'gameDetails.d3d12.confirm.backup': 'Sicherungspfad: {path}',
  'gameDetails.d3d12.confirm.backupWillCreate':
    'Vor der Änderung wird eine Sicherungskopie der ursprünglichen EXE erstellt: {path}',
  'gameDetails.d3d12.confirm.backupExists':
    'Die Original-EXE ist bereits hier gespeichert: {path}. Diese Kopie wird nicht überschrieben.',
  'gameDetails.d3d12.confirm.signatureWarning':
    'Nach der Änderung kann die digitale Signatur der EXE als ungültig gelten und eine Integritätsprüfung die Datei als geändert melden. Bei einem vollständigen Rollback von D3D12 stellt RenderPilot die ursprüngliche EXE wieder her.',
  'gameDetails.d3d12.confirm.accept': 'Ändern',
  'gameDetails.d3d12.executableLockedTitle': 'EXE-Auswahl gesperrt',
  'gameDetails.d3d12.executableLocked':
    'Um eine andere EXE auszuwählen, setzen Sie die D3D12-Komponente vollständig zurück.',
  'gameDetails.d3d12.executableRepairLocked':
    'Führen Sie die Wiederherstellung gemäß den Anweisungen in der D3D12-Karte durch und scannen Sie das Spiel anschließend erneut.',
  'gameDetails.executable.description':
    'Die Programmdatei des Spiels — das NVIDIA-Profil gilt für sie, und RenoDX wird in ihren Ordner installiert.',
  'gameDetails.executable.triggerLabel': 'Ausführbare Spieldatei: {fileName}',
  'gameDetails.executable.detectedGroup': 'Erkannte Spieldateien',
  'gameDetails.executable.otherGroup': 'Sonstige (Launcher, Installer, Tools)',
  'gameDetails.executable.customBadge': 'Manuell',
  'gameDetails.executable.reset': 'Auf automatische Erkennung zurücksetzen',
  'gameDetails.executable.tooltipAuto':
    'Spiel-Programmdatei: automatisch erkannt. Wird vom NVIDIA-Profil und RenoDX verwendet.',
  'gameDetails.executable.tooltipCustom':
    'Spiel-Programmdatei: manuell ausgewählt. Wird vom NVIDIA-Profil und RenoDX verwendet.',
  'gameDetails.profile.title': 'NVIDIA Profil',
  'gameDetails.profile.description':
    'Konfigurieren Sie die NVIDIA-Treibereinstellungen für dieses Spiel.',
  'gameDetails.profile.pinnedManual': 'Manuell ausgewählt.',
  'gameDetails.profile.autoDetected': 'Automatisch erkannt.',
  'gameDetails.profile.noExeDetected': 'Keine ausführbare Datei für dieses Spiel gefunden.',
  'gameDetails.profile.noExe': 'Keine ausführbare Datei',
  'gameDetails.profile.noProfile': 'NVIDIA-Profil nicht gefunden.',

  'gameDetails.nvapi.requiresDriver': 'erfordert Treiber {version}+',
  'gameDetails.nvapi.unavailable': 'nicht verfügbar',
  'gameDetails.nvapi.resetDefault': 'Auf Standard zurücksetzen',
  'gameDetails.nvapi.alreadyDefault': 'Bereits Standard',
  'gameDetails.nvapi.restoreBaselineLabel': 'Anfangswert wiederherstellen',
  'gameDetails.nvapi.restoreBaseline': 'Anfangswert wiederherstellen',
  'gameDetails.nvapi.alreadyBaseline': 'Bereits auf Anfangswert',
  'gameDetails.nvapi.noBaseline': 'Kein Anfangswert gespeichert',
  'gameDetails.nvapi.versionUnavailable': 'DLSS-Version nicht verfügbar',

  'gameDetails.nvapi.warning.noDll': 'Keine DLSS-DLL im Installationsverzeichnis gefunden.',
  'gameDetails.nvapi.warning.noManifest':
    'Das Manifest enthält keinen Eintrag für diese DLL-Version.',
  'gameDetails.nvapi.warning.dllVersionUnknown':
    'Eine DLSS-DLL wurde gefunden, aber ihre Version ist nicht verfügbar.',
  'gameDetails.nvapi.warning.catalogNotReady':
    'Der Spielkatalog ist nicht bereit. Scannen Sie das Spiel erneut, bevor Sie DLL-abhängige NVIDIA-Einstellungen ändern.',
  'gameDetails.nvapi.warning.noExecutable': 'Keine ausführbare Datei für dieses Spiel gefunden.',
  'gameDetails.nvapi.warning.nvapiUnavailable': 'NVAPI nicht verfügbar.',
  'gameDetails.nvapi.warning.nvapiInitFailed': 'NVAPI-Initialisierung fehlgeschlagen.',
  'gameDetails.nvapi.warning.drsFailed': 'DRS-Sitzung konnte nicht erstellt werden.',

  // ── Operations page ──
  'operations.title': 'Vorgangsjournal',
  'operations.subtitleGame': 'Aktivitäten für {title}',
  'operations.loading': 'Laden…',
  'operations.empty': 'Noch kein Verlauf vorhanden',
  'operations.gameName': 'Spiel',
  'operations.date': 'Datum',
  'operations.status': 'Status',
  'operations.action': 'Aktion',
  'operations.libraryType': 'Bibliothekstyp',
  'operations.version': 'Version',

  'libraries.error': 'Fehler',
  'libraries.catalogFallback.title': 'Katalog nicht verfügbar',
  'libraries.catalogFallback.description':
    'Es werden nur lokal registrierte Pakete angezeigt. Dies ist nicht der vollständige Katalog.',
  'libraries.state.localOnly': 'Nur lokal',
  'libraries.state.downloaded': 'Heruntergeladen',
  'libraries.state.missing': 'Dateien fehlen',
  'libraries.state.corrupt': 'Beschädigte Dateien',
  'libraries.hash.copy': 'Hash kopieren',
  'libraries.hash.copyVersion': 'Hash für {version} kopieren',
  'libraries.hash.copied': 'Kopiert',
  'libraries.hash.failed': 'Fehler beim Kopieren',
  'libraries.hash.copiedToast': 'Hash in die Zwischenablage kopiert',
  'libraries.sort.byColumn': 'Nach {label} sortieren',
  'libraries.actions.delete': 'Löschen',
  'libraries.actions.download': 'Herunterladen',
  'libraries.actions.deleteVersion': '{version} löschen',
  'libraries.actions.downloadVersion': '{version} herunterladen',
  'libraries.actions.deletedToast': '{version} gelöscht',
  'libraries.actions.downloadedToast': '{version} heruntergeladen',
  'libraries.actions.failedToast': 'Fehler bei: {action}',
  'libraries.actions.downloadAll': 'Neueste herunterladen',
  'libraries.actions.downloadAllCount': 'Neueste herunterladen ({count})',
  'libraries.actions.downloadAllUpToDate': 'Alle neuesten Versionen sind bereits heruntergeladen',
  'libraries.actions.downloadAllTooltip': plural('count', {
    one: '1 neueste Version herunterladen',
    other: '{count} neueste Versionen herunterladen',
  }),
  'libraries.actions.downloadAllDoneToast': plural('count', {
    one: '{count} Bibliothek heruntergeladen',
    other: '{count} Bibliotheken heruntergeladen',
  }),
  'libraries.actions.downloadAllPartialToast':
    '{succeeded} heruntergeladen, {failed} fehlgeschlagen',
  'libraries.actions.downloadAllNoneToast': 'Alle neuesten Versionen sind bereits heruntergeladen',
  'libraries.filters.vendorLabel': 'Bibliotheksanbieter',
  'libraries.filters.typeLabel': 'Bibliothekstypen',
  'libraries.table.caption': '{vendor}-{type}-Bibliotheksversionen',

  'common.cancel': 'Abbrechen',
  'common.apply': 'Anwenden',

  'filters.title': 'Filter',
  'filters.launchers.title': 'Launcher',
  'filters.launchers.empty': 'Keine Launcher gefunden',
  'filters.launchers.reorder.instructions':
    'Zum Sortieren fokussieren Sie die Verschieben-Schaltfläche, drücken Leertaste oder Eingabetaste, verwenden die Pfeiltasten und drücken zum Ablegen erneut Leertaste oder Eingabetaste. Escape bricht ab.',
  'filters.launchers.reorder.move': '{label} verschieben, Position {position} von {total}',
  'filters.launchers.reorder.zoneLabel': 'Zone zum Sortieren der Launcher',
  'filters.launchers.reorder.itemLabel': 'Launcher {label}',
  'filters.launchers.reorder.dragStarted':
    '{itemLabel} in {zoneLabel} aufgenommen, Position {position} von {count}.',
  'filters.launchers.reorder.movedToPosition':
    '{itemLabel} an Position {position} von {count} verschoben.',
  'filters.launchers.reorder.movedToZoneStart':
    '{itemLabel} an den Anfang von {zoneLabel} verschoben.',
  'filters.launchers.reorder.movedToZoneEnd': '{itemLabel} an das Ende von {zoneLabel} verschoben.',
  'filters.launchers.reorder.droppedAnnouncement':
    '{itemLabel} in {zoneLabel} abgelegt, Position {position} von {count}.',
  'filters.launchers.reorder.zoneActiveInstruction':
    'Leertaste oder Eingabetaste zum Aufnehmen, danach Pfeiltasten zum Verschieben verwenden.',
  'filters.launchers.reorder.zoneDragDisabledInstruction':
    'Das Sortieren der Launcher ist nicht verfügbar.',
  'filters.launchers.reorder.pickedUp': '{label} aufgenommen, Position {position} von {total}.',
  'filters.launchers.reorder.moved': '{label} auf Position {position} von {total} verschoben.',
  'filters.launchers.reorder.dropped': '{label} auf Position {position} von {total} abgelegt.',
  'filters.launchers.reorder.cancelled': 'Sortieren von {label} abgebrochen.',
  'filters.libraries.title': 'Komponenten',
  'filters.libraries.empty': 'Keine Komponenten gefunden',
  'filters.addons.title': 'Add-ons',

  'games.favoritesToggle': 'Favoriten',
  'games.favoritesToggleActive': 'Favoriten (aktiv)',
  'games.showHidden': 'Anzeigen',
  'games.showHiddenActive': 'Ausgeblendete Spiele (aktiv)',

  'operation.label.low': 'Geringes Risiko',
  'operation.label.medium': 'Mittleres Risiko',
  'operation.label.high': 'Hohes Risiko',
  'operation.label.blocked': 'Blockiert',
  'operation.label.planned': 'Geplant',
  'operation.label.completed': 'Abgeschlossen',
  'operation.label.failed': 'Fehlgeschlagen',
  'operation.label.rolledBack': 'Zurückgesetzt',
  'operation.label.replaceComponent': 'Version ändern',
  'operation.duration': 'Abgeschlossen in {duration}',
  'operation.filesUpdated.none': 'Keine Dateien aktualisiert.',
  'operation.filesUpdated.count': plural('count', {
    one: '1 Datei aktualisiert.',
    other: '{count} Dateien aktualisiert.',
  }),
  'operation.filesRestored.none': 'Keine Dateien wiederhergestellt.',
  'operation.filesRestored.count': plural('count', {
    one: '1 Datei wiederhergestellt.',
    other: '{count} Dateien wiederhergestellt.',
  }),
  'operation.itemLabel': '{kind}, {status}',

  'notify.stalePlan': 'Der Vorgangsplan ist veraltet. Bitte versuchen Sie es erneut.',
  'notify.missingStableGameId': 'Das Spiel konnte nicht identifiziert werden.',
  'notify.coverPickerPreview': 'Bitte verwenden Sie die Desktop-App, um ein Cover auszuwählen.',
  'notify.coverUpdated.title': 'Cover aktualisiert',
  'notify.coverUpdated.body': 'Ihr benutzerdefiniertes Cover wurde gespeichert.',
  'notify.coverDownloaded.title': 'Cover heruntergeladen',
  'notify.coverDownloaded.body': 'Das Spiel-Cover wurde aktualisiert.',
  'notify.coverRemoved.title': 'Cover entfernt',
  'notify.coverRemoved.body': 'Standard-Cover wiederhergestellt.',
  'notify.favoriteFailed': 'Favoritenstatus konnte nicht geändert werden.',
  'notify.favoriteAdded': 'Zu Favoriten hinzugefügt.',
  'notify.favoriteRemoved': 'Aus Favoriten entfernt.',
  'notify.hiddenFailed': 'Versteckt-Status konnte nicht geändert werden.',
  'notify.gameHidden': 'Spiel ausgeblendet.',
  'notify.gameUnhidden': 'Spiel eingeblendet.',
  'notify.gameRemovedFromCatalog': 'Spiel aus dem Katalog entfernt.',
  'notify.removeGameFailed': 'Das Spiel konnte nicht aus dem Katalog entfernt werden.',
  'notify.applyCompleted': 'Änderungen angewendet',
  'notify.rollbackCompleted': 'Rollback abgeschlossen',
  'notify.swapBatchFailed.title': 'Einige Aktualisierungen fehlgeschlagen',
  'notify.swapBatchFailed.description':
    '{failed} von {total} Komponenten konnten nicht aktualisiert werden.',
  'notify.rollbackBatchFailed.title': 'Einige Wiederherstellungen fehlgeschlagen',
  'notify.rollbackBatchFailed.description':
    '{failed} von {total} Komponenten konnten nicht wiederhergestellt werden.',
  'notify.statusError': 'Fehler',
  'notify.statusWarning': 'Warnung',

  'scan.partialWarning': plural('count', {
    one: '1 Ordner konnte nicht gescannt werden.',
    other: '{count} Ordner konnten nicht gescannt werden.',
  }),
  'scan.automaticFailed':
    'Die automatische Bibliothekssuche ist fehlgeschlagen. Die Spieleliste wurde trotzdem aktualisiert.',

  'coverSync.failed': 'Cover konnten nicht synchronisiert werden.',
  'coverSync.refreshFailed': 'Cover konnten nicht synchronisiert werden.',
  'coverSync.failure.single':
    'Das Cover für {title} konnte nicht heruntergeladen werden: {message}',
  'coverSync.failure.multiple': plural('count', {
    one: 'Cover für {count} Spiel konnten nicht heruntergeladen werden. Erster Fehler: {summary}',
    other:
      'Cover für {count} Spiele konnten nicht heruntergeladen werden. Erster Fehler: {summary}',
  }),
  'coverSync.failure.hint': 'Prüfen Sie die Cover-Quellen und die SteamGridDB-Einstellungen.',

  'nvidia.changeSettingFailed': 'Einstellungen konnten nicht angewendet werden',
  'nvidia.revertDefaultFailed': 'Standardeinstellungen konnten nicht wiederhergestellt werden',
  'nvidia.revertBaselineFailed': 'Anfangseinstellungen konnten nicht wiederhergestellt werden',

  'indicator.changeFailed': 'DLSS-Indikator konnte nicht umgeschaltet werden',

  'libraries.column.version': 'Version',
  'libraries.column.hash': 'Hash',
  'libraries.column.signed': 'Signiert',
  'libraries.column.size': 'Größe',
  'libraries.column.documents': 'Dokumente',
  'libraries.column.actions': 'Aktionen',
  'libraries.documents.openForVersion': 'Rechtsdokumente für {name} {version} öffnen',
  'libraries.documents.title': 'Rechtsdokumente',
  'libraries.documents.description': 'Gilt für {name} {version}.',
  'libraries.documents.formatPdf': 'PDF',
  'libraries.documents.formatText': 'Text',
  'libraries.documents.open': 'Öffnen',
  'libraries.documents.openFailed': 'Dokument konnte nicht geöffnet werden',
  'libraries.unsigned': 'Nicht signiert',
  'libraries.invalidDate': 'Ungültiges Datum',
  'libraries.empty.loading': 'Wird geladen…',
  'libraries.empty.unavailable': 'Bibliotheken konnten nicht geladen werden',
  'libraries.empty.none': 'Keine Bibliotheken gefunden',
  'libraries.error.loadFailed': 'Bibliotheken konnten nicht geladen werden',
  'libraries.error.refreshFailed': 'Manifest konnte nicht aktualisiert werden',
  'libraries.error.downloadFailed': 'Download fehlgeschlagen',
  'libraries.error.deleteFailed': 'Löschen fehlgeschlagen',
  'libraries.error.downloadedRefreshFailed':
    'Bibliothek heruntergeladen, aber Statusaktualisierung fehlgeschlagen',
  'libraries.error.deletedRefreshFailed':
    'Bibliothek gelöscht, aber Statusaktualisierung fehlgeschlagen',

  'settings.catalog.source.steam.actionLabel': 'Cover von Steam herunterladen',
  'settings.catalog.source.steam.title': 'Steam',
  'settings.catalog.source.steam.description':
    'Cover aus dem öffentlichen Steam-Katalog herunterladen.',
  'settings.catalog.source.gog.actionLabel': 'Cover von GOG herunterladen',
  'settings.catalog.source.gog.title': 'GOG',
  'settings.catalog.source.gog.description': 'Cover aus dem offiziellen GOG-Katalog herunterladen.',
  'settings.catalog.source.steamgriddb.actionLabel': 'Cover von SteamGridDB herunterladen',
  'settings.catalog.source.steamgriddb.title': 'SteamGridDB',
  'settings.catalog.source.steamgriddb.description':
    'Community-Cover von SteamGridDB herunterladen. Erfordert API-Schlüssel.',
  'settings.catalog.artworkReadError': 'Cover-Einstellungen konnten nicht geladen werden.',
  'settings.catalog.artworkSaveError': 'Cover-Einstellungen konnten nicht gespeichert werden.',

  'user_message.invalid_argument': 'Ungültige Eingabe bereitgestellt.',
  'user_message.invalid_install_root':
    'Wählen Sie den Installationsordner eines einzelnen Spiels. Laufwerkswurzeln, Netzwerkfreigabewurzeln und Systemordner können nicht hinzugefügt werden.',
  'user_message.multiple_installs_detected':
    'Dieser Ordner enthält mehrere Spielinstallationen. Wähle den Installationsordner eines einzelnen Spiels.',
  'user_message.stale_install_inspection':
    'Die Installation hat sich während der Prüfung geändert. Prüfen Sie das aktualisierte Ergebnis vor dem Hinzufügen.',
  'user_message.root_correction_cleanup_required':
    'Aktive Komponentenänderungen müssen rückgängig gemacht werden, bevor dieser Spielordner geändert werden kann.',
  'user_message.root_correction_blocked':
    'Bereinigen Sie den aktiven Spielzustand in der vorhandenen Karte, bevor Sie den Stammordner ändern.',
  'user_message.managed_cleanup_ambiguous':
    'RenderPilot hat sich überschneidende verwaltete Änderungen gefunden, deren sichere Rücksetzreihenfolge nicht belegt werden kann. Es wurde nichts geändert und ein Wiederherstellungspaket wurde erstellt.',
  'user_message.catalog_consolidation_blocked':
    'RenderPilot hat widersprüchliche verwaltete Zustände in doppelten Spielkarten gefunden. Es wurde nichts geändert und ein Wiederherstellungspaket erstellt.',
  'user_message.game_removal_cleanup_failed':
    'RenderPilot konnte die ursprünglichen Spieldateien nicht wiederherstellen. Die Karte wurde daher nicht entfernt. Prüfen Sie die Spieldateien und versuchen Sie es erneut.',
  'user_message.invalid_game_reference': 'Spiel nicht gefunden.',
  'user_message.invalid_component_reference': 'Komponente nicht gefunden.',
  'user_message.invalid_artifact_reference': 'Element nicht gefunden.',
  'user_message.invalid_operation_reference': 'Aktion nicht gefunden.',
  'user_message.response_serialization_failed': 'Fehler bei der Verarbeitung der Anfrage.',
  'user_message.plan_changed_rebuild': 'Die Aufgabe ist veraltet. Bitte versuchen Sie es erneut.',
  'user_message.game_not_in_catalog': 'Das Spiel wird nicht unterstützt.',
  'user_message.operation_not_found': 'Aktion nicht gefunden.',
  'user_message.artifact_not_found': 'Element nicht gefunden.',
  'user_message.component_not_found': 'Komponente nicht gefunden.',
  'user_message.invalid_operation_state': 'Diese Aktion ist derzeit nicht verfügbar.',
  'user_message.operation_could_not_complete': 'Aktion konnte nicht abgeschlossen werden.',
  'user_message.rollback_also_failed':
    'Die Aktion ist fehlgeschlagen und RenderPilot konnte den vorherigen Dateistand nicht vollständig wiederherstellen. Prüfen Sie die Spieldateien, bevor Sie es erneut versuchen.',
  'user_message.command_task_failed': 'Befehl konnte nicht ausgeführt werden.',
  'user_message.storage_failed':
    'Der Katalog der App konnte nicht gelesen oder geschrieben werden.',
  'user_message.provider_failed': 'Eine Datenquelle konnte nicht gelesen werden.',
  'user_message.detection_failed': 'Die Spieldateien konnten nicht analysiert werden.',
  'user_message.steamgriddb_api_key_missing':
    'Bitte geben Sie in den Einstellungen einen SteamGridDB API-Schlüssel ein.',
  'user_message.unsupported_cover_image_type': 'Nicht unterstütztes Bildformat.',
  'user_message.cover_download_failed': 'Cover konnte nicht heruntergeladen werden.',
  'user_message.cover_artwork_not_found': 'Kein Cover für dieses Spiel gefunden.',
  'user_message.cover_file_system_error':
    'Cover konnte nicht auf der Festplatte gespeichert werden.',
  'user_message.stale_replacement_source':
    'Dieses Update konnte nicht angewendet werden, weil die Quelldatei außerhalb von RenderPilot ersetzt oder geändert wurde. Bitte wählen Sie die Version erneut — möglicherweise ist ein Download erforderlich.',
  'user_message.access_denied':
    'Zugriff verweigert. Überprüfen Sie Ihre Berechtigungen und versuchen Sie es erneut.',
  'user_message.nvapi_catalog_not_ready':
    'Scannen Sie das Spiel erneut, bevor Sie DLL-abhängige NVIDIA-Einstellungen ändern.',

  'suggested_action.refresh_games':
    'Aktualisieren Sie die Spieleliste und versuchen Sie es erneut.',
  'suggested_action.reload_game_details':
    'Aktualisieren Sie die Spieldetails und versuchen Sie es erneut.',
  'suggested_action.refresh_candidates': 'Aktualisieren Sie die Liste und versuchen Sie es erneut.',
  'suggested_action.rebuild_plan_or_reload_operations':
    'Aktualisieren Sie die Ansicht und versuchen Sie es erneut.',
  'suggested_action.retry_after_required_data':
    'Bitte warten Sie und versuchen Sie es später noch einmal.',
  'suggested_action.inspect_logs':
    'Wenn das Problem weiterhin besteht, versuchen Sie, die App neu zu starten.',
  'suggested_action.retry_or_restart':
    'Wenn das Problem weiterhin besteht, versuchen Sie, die App neu zu starten.',
  'suggested_action.rebuild_operation_plan': 'Bitte starten Sie die Aktion neu.',
  'suggested_action.refresh_or_scan_game_folder':
    'Aktualisieren Sie die Liste oder scannen Sie den Ordner erneut.',

  'settings.about.title': 'Updates',
  'settings.about.description': 'Nach Updates für die Anwendung suchen.',
  'settings.about.version.title': 'App-Version',
  'settings.about.version.loading': 'Wird geladen…',
  'settings.about.checkForUpdates': 'Nach Updates suchen',
  'settings.about.updateInProgress': 'Wird aktualisiert…',
  'settings.about.updateAvailable': 'Update verfügbar',
  'settings.about.upToDate': 'Sie haben die neueste Version',
  'settings.about.updateCheckError': 'Fehler bei der Suche nach Updates',

  'settings.about.updateDialog.title': 'Update verfügbar',
  'settings.about.updateDialog.versionLine': '{currentVersion} → {version}',
  'settings.about.updateDialog.releaseDate': 'Veröffentlicht am {date}',
  'settings.about.updateDialog.releaseNotes': 'Versionshinweise',
  'settings.about.updateDialog.noNotes':
    'Für dieses Update wurden keine Versionshinweise bereitgestellt.',
  'settings.about.updateDialog.notesTruncated': 'Die Versionshinweise wurden gekürzt.',

  'settings.about.updateDialog.installAndRestart': 'Installieren und neu starten',
  'settings.about.updateDialog.later': 'Später',
  'settings.about.updateDialog.close': 'Schließen',
  'settings.about.updateDialog.retryDownload': 'Download wiederholen',
  'settings.about.updateDialog.retryInstall': 'Installation wiederholen',
  'settings.about.updateDialog.restartNow': 'Jetzt neu starten',

  'settings.about.updateDialog.downloading': 'Update wird heruntergeladen…',
  'settings.about.updateDialog.downloadingBytes': '{received} heruntergeladen',
  'settings.about.updateDialog.downloadingBytesTotal': '{received} von {total}',
  'settings.about.updateDialog.verifying': 'Update wird überprüft…',
  'settings.about.updateDialog.verifyingDescription': 'Das heruntergeladene Paket wird geprüft.',
  'settings.about.updateDialog.installing':
    'Update wird angewendet… Die App wird geschlossen und automatisch neu gestartet.',
  'settings.about.updateDialog.restarting': 'Anwendung wird neu gestartet…',

  'settings.about.updateDialog.prepareErrorTitle': 'Download oder Überprüfung fehlgeschlagen',
  'settings.about.updateDialog.prepareErrorDescription':
    'Das Update konnte nicht heruntergeladen oder überprüft werden. Prüfen Sie die Verbindung und versuchen Sie es erneut.',
  'settings.about.updateDialog.installErrorTitle': 'Installation fehlgeschlagen',
  'settings.about.updateDialog.installErrorDescription':
    'Das Update konnte nicht installiert werden. Starten Sie RenderPilot normal neu und versuchen Sie es erneut; Windows fordert bei Bedarf Administratorrechte an.',
  'settings.about.updateDialog.restartRequiredTitle': 'Neustart erforderlich',
  'settings.about.updateDialog.restartRequiredDescription':
    'Das Update wurde installiert, aber die Anwendung konnte nicht automatisch neu gestartet werden. Starten Sie RenderPilot manuell neu, um das Update abzuschließen.',

  'settings.about.updateDialog.progressLabel': 'Downloadfortschritt',
  // ── Game details: RenoDX ──
  'gameDetails.renodx.title': 'RenoDX HDR',
  'gameDetails.renodx.description':
    'Füge diesem Spiel über das RenoDX-ReShade-Add-on HDR und Tone-Mapping hinzu.',
  'gameDetails.renodx.loading': 'Verfügbarkeit wird geprüft…',
  'gameDetails.renodx.installError': 'RenoDX-Installation fehlgeschlagen',
  'gameDetails.renodx.uninstallError': 'Entfernen von RenoDX fehlgeschlagen',
  'gameDetails.renodx.switchError': 'ReShade-Kanalwechsel fehlgeschlagen',
  'gameDetails.renodx.unsupported': 'Für dieses Spiel ist kein RenoDX-Profil verfügbar.',
  'gameDetails.renodx.incompatible': 'RenoDX kann nicht installiert werden: {reason}.',
  'gameDetails.renodx.status.label': 'Status',
  'gameDetails.renodx.statusInstalled': 'Installiert',
  'gameDetails.renodx.actionInstall': 'Installieren',
  'gameDetails.renodx.actionUninstall': 'RenoDX entfernen',
  'gameDetails.renodx.actionRepair': 'Reparieren',
  'gameDetails.renodx.actionRepairDlssFix': 'DLSS-Fix reparieren',
  'gameDetails.renodx.actionFinishDlssFixRecovery': 'Wiederherstellung abschließen',
  'gameDetails.renodx.dlssFixRecoveryPending':
    'Ein vorheriger DLSS-Fix-Vorgang muss wiederhergestellt werden.',
  'gameDetails.renodx.uninstallConfirmTitle': 'RenoDX aus diesem Spiel entfernen?',
  'gameDetails.renodx.uninstallConfirmBody':
    'Dies entfernt das RenoDX-Add-on und stellt nur ReShade-Dateien wieder her, die während der RenoDX-Einrichtung geändert wurden.',
  'gameDetails.renodx.uninstallConfirmAction': 'Entfernen',
  'gameDetails.renodx.installing': 'Installation…',
  'gameDetails.renodx.confirmTitle': 'RenoDX trotz Anti-Cheat-Risiko installieren?',
  'gameDetails.renodx.cancel': 'Abbrechen',
  // ── Game details: RenoDX shared Vulkan layer ──
  'gameDetails.renodx.vulkanLayer.removeError':
    'Die gemeinsam genutzte ReShade-Vulkan-Ebene konnte nicht entfernt werden.',
  'gameDetails.renodx.vulkanLayer.title': 'Gemeinsam genutzte Vulkan-Ebene',
  'gameDetails.renodx.vulkanLayer.removeConfirmTitle': 'Geteilte Vulkan-Ebene entfernen?',
  'gameDetails.renodx.vulkanLayer.removeConfirmBody':
    'Das Entfernen der gemeinsam genutzten ReShade Vulkan-Ebene betrifft alle Vulkan RenoDX-Spiele. Fortfahren?',
  'gameDetails.renodx.vulkanLayer.openSettings': 'RenoDX-Einstellungen öffnen',
  'gameDetails.renodx.vulkanLayer.externalReadOnly':
    'Vorhandene Vulkan-Ebene erkannt; in dieser Version schreibgeschützt',
  'gameDetails.renodx.vulkanLayer.state.not_installed': 'Nicht installiert',
  'gameDetails.renodx.vulkanLayer.state.installed': 'Installiert',
  'gameDetails.renodx.vulkanLayer.state.installed_disabled': 'Disabled in registry',
  'gameDetails.renodx.vulkanLayer.state.external_read_only': 'Schreibgeschützt',
  'gameDetails.renodx.vulkanLayer.state.conflict': 'Konflikt',
  'gameDetails.renodx.vulkanLayer.state.needs_repair': 'Reparatur nötig',
  'gameDetails.renodx.vulkanLayer.state.unsupported': 'Nicht unterstützt',
  'gameDetails.renodx.vulkanLayer.action.install': 'Installieren',
  'gameDetails.renodx.vulkanLayer.action.update': 'Aktualisieren',
  'gameDetails.renodx.vulkanLayer.action.switch_channel': 'Kanal wechseln',
  'gameDetails.renodx.vulkanLayer.action.repair': 'Layer reparieren',
  'gameDetails.renodx.vulkanLayer.action.remove': 'Entfernen',
  'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected':
    'Es wurde eine vorhandene Vulkan-Ebene erkannt.',
  'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest':
    'Es sind mehrere ReShade-Ebenen-Manifeste registriert.',
  'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility':
    'Die Sichtbarkeit des Loaders ist mehrdeutig.',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll': 'Die DLL der Ebene fehlt.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll':
    'The layer DLL could not be read (permission denied or locked).',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest': 'The layer manifest is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing':
    'Layer-Dateien sind vorhanden, aber die Vulkan-Loader-Registrierung fehlt.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled':
    'The loader registry entry is disabled.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture':
    'Die Architektur der Ebene wird nicht unterstützt.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated':
    'Die Ebene ist unter HKCU registriert und wird für Spiele mit erhöhten Rechten möglicherweise nicht geladen.',
  'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed':
    'Ein Ebenen-Manifest konnte nicht analysiert werden.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable':
    'Der erforderliche Registrierungsbereich kann nicht beschrieben werden.',
  'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied':
    'Das Betriebssystem hat einen erforderlichen Vorgang abgelehnt.',
  'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed':
    'Backend-Validierung fehlgeschlagen; die Ebene muss geprüft werden.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch':
    'The layer DLL hash does not match the expected version.',
  'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback':
    'The layer DLL is missing; using advisory database record.',
  // ── Game details: RenoDX incompatibility reasons ──
  'gameDetails.renodx.reason.api_unsupported': 'nicht unterstützte Grafik-API',
  'gameDetails.renodx.reason.api_not_allowed': 'Grafik-API für dieses Spiel nicht zulässig',
  'gameDetails.renodx.reason.arch_unknown': 'unbekannte Architektur der ausführbaren Datei',
  'gameDetails.otherTab': 'Sonstiges',
  'gameDetails.renodx.unavailable': 'RenoDX ist derzeit nicht verfügbar.',
  'renodx.generic.universal': 'Universal-RenoDX',
  'renodx.generic.unity': 'Universal-RenoDX (Unity)',
  'gameDetails.renodx.generic.profileTooltip': 'Ein gemeinsames Engine-Profil wird verwendet.',
  'renodx.phase.finalizing': 'Abschluss…',
  'luma.phase.finalizing': 'Abschluss…',
  'gameDetails.renodx.confidenceLabel': 'RenoDX-Kompatibilität',
  'gameDetails.renodx.confidenceVerified': 'Funktioniert',
  'gameDetails.renodx.confidenceExperimental': 'In Arbeit',
  'gameDetails.renodx.confidenceUntested': 'Ungeprüft',
  'gameDetails.renodx.external':
    'Dieses RenoDX-Add-on wird extern verteilt und muss manuell heruntergeladen werden.',
  'gameDetails.renodx.actionOpenExternal': 'Download-Seite öffnen',
  'gameDetails.renodx.external.installFromFile': 'Aus Datei installieren',
  'gameDetails.renodx.external.dropHint':
    'Lade das Add-on herunter und ziehe es hierher oder wähle die Datei aus.',
  'gameDetails.renodx.external.invalidFile':
    'Diese Datei ist kein RenoDX-Add-on (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.title': 'Manuelle Installation',
  'gameDetails.renodx.fileInstall.chooseFile': 'Add-on-Datei wählen…',
  'gameDetails.renodx.fileInstall.chooseAnother': 'Andere Datei wählen',
  'gameDetails.renodx.fileInstall.expected': 'Erwartetes Add-on: {name}',
  'gameDetails.renodx.fileInstall.confirm': '{fileName} installieren?',
  'gameDetails.renodx.fileInstall.errorExtension':
    'Diese Datei ist kein RenoDX-Add-on (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.errorArch':
    'Dieses Add-on ist {addon}, das Spiel aber {game}. Lade das passende Add-on herunter.',
  'gameDetails.renodx.fileInstall.warnName':
    'Das sieht nicht nach dem erwarteten Add-on aus ({expected}). Nur installieren, wenn du sicher bist.',
  'gameDetails.renodx.nativeHdr':
    'Dieses Spiel unterstützt bereits natives HDR – RenoDX wird nicht benötigt.',
  'gameDetails.renodx.blacklisted': 'RenoDX wird für dieses Spiel nicht empfohlen.',
  'gameDetails.renodx.updatesNotTracked': 'Updates werden nicht verfolgt',
  'gameDetails.renodx.channel.label': 'ReShade-Host-Kanal',
  'gameDetails.renodx.channel.hostLabel': 'ReShade-Host',
  'gameDetails.renodx.channel.stable': 'Stable',
  'gameDetails.renodx.channel.nightly': 'Nightly',
  'gameDetails.renodx.host.version': '{version}',
  'gameDetails.renodx.host.versionUnknown': 'Version unbekannt',
  'gameDetails.renodx.host.addons.none': 'Add-ons nicht unterstützt',
  'gameDetails.renodx.host.addons.unknown': 'Add-on-Unterstützung unbekannt',
  'gameDetails.renodx.host.action.update_host': 'Update verfügbar',
  'gameDetails.renodx.host.action.repair_host':
    'ReShade für RenoDX-Add-on-Unterstützung reparieren',
  'gameDetails.renodx.host.customBuild':
    'Custom-Build (z. B. GShade) — Sie aktualisieren es selbst',
  'gameDetails.renodx.host.conflictMultiple':
    'Mehrere ReShade-Hosts gefunden – aktiver Slot muss geprüft werden',
  'gameDetails.renodx.host.conflictBlocksInstall':
    'Eine vorhandene Datei belegt den ReShade-Slot dieses Spiels, oder ReShade liegt in einem anderen Slot – vor der Installation beheben.',
  'gameDetails.renodx.actionUpdate': 'Aktualisieren',
  'gameDetails.renodx.updating': 'Wird aktualisiert…',
  'gameDetails.renodx.updateError': 'RenoDX-Aktualisierung fehlgeschlagen',
  'gameDetails.renodx.actionInstallDlssFix': 'Installieren',
  'gameDetails.renodx.actionRemoveDlssFix': 'Entfernen',
  'gameDetails.renodx.dlssFixInstallError': 'DLSS-Fix Installation fehlgeschlagen',
  'gameDetails.renodx.dlssFixRemoveError': 'DLSS-Fix Entfernung fehlgeschlagen',
  'gameDetails.renodx.fresh.label': 'Updates',
  'gameDetails.renodx.fresh.current': 'Aktuell',
  'gameDetails.renodx.fresh.available': 'Update verfügbar',
  'gameDetails.renodx.fresh.channelMismatch': 'Kanalwechsel verfügbar',
  'gameDetails.renodx.fresh.validationRequired': 'Überprüfung erforderlich',
  'gameDetails.renodx.fresh.unknown': 'Konnte nicht prüfen',
  'gameDetails.renodx.fresh.checking': 'Wird geprüft…',
  'gameDetails.renodx.addonDated': 'Add-on vom {date}',
  'gameDetails.renodx.installedOn': 'Installiert {date}',
  'gameDetails.renodx.lastChecked': 'Geprüft {time}',
  'gameDetails.renodx.lastCheckedNever': 'Noch nicht geprüft',
  'gameDetails.renodx.actionCheckUpdates': 'Nach Updates suchen',
  'gameDetails.renodx.component.reshade': 'ReShade-Host',
  'gameDetails.renodx.component.addon': 'RenoDX-Add-on',
  'gameDetails.renodx.component.addonDesc': 'Das HDR-Add-on für dieses Spiel',
  'gameDetails.renodx.component.addonDisabled': 'Installiert, aber in ReShade.ini deaktiviert',
  'gameDetails.renodx.component.addonFileInstall':
    'Aus Datei installiert — nicht auf Updates überwacht',
  'gameDetails.renodx.component.dlssFix': 'DLSS-Fix',
  'gameDetails.renodx.component.dlssFixDesc': 'Behebt Flackern bei der DLSS Frame-Erstellung',
  'gameDetails.renodx.component.dlssFixOffer':
    'Verfügbar — verhindert Flackern bei der DLSS Frame-Erstellung',
  'gameDetails.renodx.component.dlssFixHint':
    'Ein allgemeiner ReShade-Fix, nicht RenoDX-spezifisch. ReShade zeichnet auf den nativen Frames des Spiels statt auf den Frame-Generation-Frames und blendet DLSS-Upscaling vor ReShade aus, wenn das Spiel Streamline korrekt umsetzt.',
  'gameDetails.renodx.attribution': 'RenoDX von clshortfuse.',
  'gameDetails.renodx.attributionLink': 'Projekt ansehen',
  // ── Game details: shared add-on copy (RenoDX + Luma) ──
  'gameDetails.addon.riskSafe': 'Kein Anti-Cheat erkannt — Installation ist sicher.',
  'gameDetails.addon.riskWarn':
    'Anti-Cheat erkannt — die Installation kann zu einer Sperre führen.',
  'addon.risk.sp_safe':
    'Keine bekannten Anti-Cheat-Signaturen gefunden — die Installation von {addonName} ist wahrscheinlich sicher, aber nicht garantiert.',
  'addon.risk.anticheat_detected':
    'Anti-Cheat-Signaturen erkannt — die Installation von {addonName} kann zu einer Sperre führen.',
  'gameDetails.addon.confirmAccept': 'Trotzdem installieren',
  'gameDetails.addon.confirmBody':
    'Dieses Spiel verwendet Anti-Cheat. Das ReShade-Add-on könnte es auslösen und zu einer Sperre führen. Fahren Sie auf eigenes Risiko fort.',
  'gameDetails.addon.fullAddonWarning':
    'Volle ReShade-Add-on-Unterstützung kann bei Mehrspieler- oder Anti-Cheat-geschützten Spielen unsicher sein.',
  'gameDetails.addon.blockedByOtherAddon.tracked':
    '{installedAddon} ist für dieses Spiel installiert — deinstallieren Sie es, bevor Sie {blockedAddon} installieren.',
  'gameDetails.addon.blockedByOtherAddon.unmanaged':
    'Für dieses Spiel wurden {installedAddon}-Dateien auf der Festplatte gefunden — entfernen Sie sie, bevor Sie {blockedAddon} installieren.',
  'addon.availability.loadFailed': 'Konnte nicht geprüft werden',
  'addon.availability.retry': 'Erneut versuchen',
  'addon.availability.checking': 'Wird geprüft…',
  // ── Game details: Luma ──
  'gameDetails.luma.title': 'Luma Framework',
  'gameDetails.luma.description':
    'Die für dieses Spiel verfügbaren Luma-Funktionen sind unten aufgeführt.',
  'gameDetails.luma.loading': 'Verfügbarkeit wird geprüft…',
  'gameDetails.luma.installError': 'Luma-Installation fehlgeschlagen',
  'gameDetails.luma.uninstallError': 'Luma-Deinstallation fehlgeschlagen',
  'gameDetails.luma.updateError': 'Luma-Update fehlgeschlagen',
  'gameDetails.luma.repairError': 'Luma-Reparatur fehlgeschlagen',
  'gameDetails.luma.unsupported': 'Für dieses Spiel ist kein Luma-Profil verfügbar.',
  'gameDetails.luma.incompatible': 'Luma kann nicht installiert werden: {reason}.',
  'gameDetails.luma.blacklisted': 'Luma wird für dieses Spiel nicht empfohlen.',
  'gameDetails.luma.unavailable': 'Luma ist derzeit nicht verfügbar.',
  'gameDetails.luma.unmanagedPresent':
    'Auf der Festplatte wurde eine bestehende Luma-Installation ohne verfolgten Datensatz gefunden. Entfernen Sie sie manuell und installieren Sie dann neu.',
  'gameDetails.luma.installTornWarning':
    'Eine vorherige Installation wurde nicht sauber abgeschlossen. Eine erneute Installation bereinigt und repariert sie.',
  'gameDetails.luma.installTornWarningInstalled':
    'Der letzte Vorgang wurde nicht sauber abgeschlossen. Verwenden Sie „Reparieren“ (oder „Aktualisieren“, falls angezeigt), um die Installation abzuschließen.',
  'gameDetails.luma.status.label': 'Status',
  'gameDetails.luma.statusInstalled': 'Installiert',
  'gameDetails.luma.actionInstall': 'Installieren',
  'gameDetails.luma.installing': 'Wird installiert…',
  'gameDetails.luma.actionUninstall': 'Luma entfernen',
  'gameDetails.luma.actionRepair': 'Reparieren',
  'gameDetails.luma.actionUpdate': 'Aktualisieren',
  'gameDetails.luma.updating': 'Wird aktualisiert…',
  'gameDetails.luma.actionCheckUpdates': 'Nach Updates suchen',
  'gameDetails.luma.uninstallConfirmTitle': 'Luma von diesem Spiel entfernen?',
  'gameDetails.luma.uninstallConfirmBody':
    'Luma wird entfernt. Wenn Luma die DLSS-DLL verwaltet, wird der Library Swap zurückgesetzt und der exakte Zustand vor Luma wiederhergestellt. Wiederverwendete DLLs und unabhängige Swaps bleiben unverändert.',
  'gameDetails.luma.uninstallConfirmAction': 'Entfernen',
  'gameDetails.luma.confirmTitle': 'Luma trotz Anti-Cheat-Risiko installieren?',
  'gameDetails.luma.vcredistWarning':
    'Ein aktuelles Visual C++ Redistributable scheint auf diesem System zu fehlen. Wenn Luma nicht lädt, installieren Sie das Redistributable.',
  'gameDetails.luma.vcredistLink': 'Redistributable herunterladen',
  'gameDetails.luma.dgvoodoo.managed':
    'RenderPilot installiert und konfiguriert dgVoodoo2 {version} für dieses Luma-Profil.',
  // ── Game details: Luma confidence ──
  'gameDetails.luma.confidenceLabel': 'Luma-Kompatibilität',
  'gameDetails.luma.confidenceVerified': 'Funktioniert',
  'gameDetails.luma.confidenceExperimental': 'In Arbeit',
  'gameDetails.luma.confidenceUntested': 'Ungetestet',
  'gameDetails.luma.generic.engineUnreal': 'Unreal Engine',
  'gameDetails.luma.generic.engineUnity': 'Unity',
  'gameDetails.luma.generic.profileTooltip': 'Ein gemeinsames Engine-Profil wird verwendet.',
  'gameDetails.luma.features.title': 'Funktionen',
  'gameDetails.luma.features.dlssFsr': 'DLSS / FSR',
  'gameDetails.luma.features.hdr': 'HDR',
  'gameDetails.luma.features.supported': 'Unterstützt',
  'gameDetails.luma.features.unsupported': 'Nicht unterstützt',
  'gameDetails.luma.features.experimental': 'Experimentell',
  'gameDetails.luma.features.unknown': 'Unbekannt',
  // ── Game details: Luma incompatibility reasons ──
  'gameDetails.luma.reason.api_unsupported': 'nicht unterstützte Grafik-API',
  'gameDetails.luma.reason.api_not_allowed': 'Grafik-API für dieses Spiel nicht erlaubt',
  'gameDetails.luma.reason.arch_unknown': 'unbekannte Architektur der ausführbaren Datei',
  'gameDetails.luma.reason.arch_mismatch':
    'Architektur der ausführbaren Datei stimmt nicht mit diesem Add-on überein',
  // ── Game details: Luma ReShade host ──
  'gameDetails.luma.channel.stable': 'Stable',
  'gameDetails.luma.channel.nightly': 'Nightly',
  'gameDetails.luma.host.version': '{version}',
  'gameDetails.luma.host.versionUnknown': 'Version unbekannt',
  'gameDetails.luma.host.addons.none': 'Add-ons nicht unterstützt',
  'gameDetails.luma.host.addons.unknown': 'Add-on-Unterstützung unbekannt',
  'gameDetails.luma.host.action.update_host': 'Update verfügbar',
  'gameDetails.luma.host.action.repair_host': 'ReShade für Luma-Add-on-Unterstützung reparieren',
  'gameDetails.luma.host.customBuild':
    'Individuelle Version (z. B. GShade) — Sie verwalten Updates selbst',
  'gameDetails.luma.host.conflictMultiple':
    'Mehrere ReShade-Hosts gefunden — aktiver Slot muss überprüft werden',
  'gameDetails.luma.host.conflictBlocksInstall':
    'Der von diesem Spiel verwendete ReShade-Slot ist durch eine vorhandene Datei belegt, oder ReShade befindet sich in einem anderen Slot — beheben Sie dies vor der Installation.',
  // ── Game details: Luma freshness / timestamps ──
  'gameDetails.luma.fresh.label': 'Version',
  'gameDetails.luma.fresh.current': 'Aktuell',
  'gameDetails.luma.fresh.available': 'Update verfügbar',
  'gameDetails.luma.fresh.channelMismatch': 'Kanalwechsel verfügbar',
  'gameDetails.luma.fresh.validationRequired': 'Überprüfung erforderlich',
  'gameDetails.luma.fresh.unknown': 'Prüfung nicht möglich',
  'gameDetails.luma.fresh.checking': 'Wird geprüft…',
  'gameDetails.luma.updatesNotTracked': 'Updates werden nicht verfolgt',
  'gameDetails.luma.addonDated': 'Add-on vom {date}',
  'gameDetails.luma.installedOn': 'Installiert am {date}',
  'gameDetails.luma.lastChecked': 'Geprüft {time}',
  'gameDetails.luma.lastCheckedNever': 'Noch nicht geprüft',
  // ── Game details: Luma components ──
  'gameDetails.luma.component.reshade': 'ReShade-Host',
  'gameDetails.luma.component.addon': 'Luma-Add-on',
  'gameDetails.luma.component.addonDesc': 'Luma-Funktionen für dieses Spiel',
  'gameDetails.luma.component.dgvoodoo': 'dgVoodoo2-Wrapper',
  'gameDetails.luma.component.dgvoodooDesc': 'Verwaltete D3D9-Brücke, Version {version}',
  // ── Game details: Luma launch arguments ──
  'gameDetails.luma.launchArgs.instructions.steam':
    'Wenn Sie das Spiel über Steam starten, fügen Sie sie dort hinzu: Rechtsklick auf das Spiel → Eigenschaften → Allgemein → Startoptionen.',
  'gameDetails.luma.launchArgs.instructions.gog':
    'Wenn Sie das Spiel über GOG Galaxy starten, fügen Sie sie dort hinzu: Spieleinstellungen → Installation verwalten → Konfigurieren.',
  'gameDetails.luma.launchArgs.instructions.epic':
    'Wenn Sie das Spiel über den Epic Games Launcher starten, fügen Sie sie dort hinzu: Rechtsklick auf das Spiel → Verwalten → Zusätzliche Befehlszeilenargumente.',
  'gameDetails.luma.launchArgs.instructions.ea':
    'Wenn Sie das Spiel über die EA app starten, fügen Sie sie dort hinzu: Spiel auswählen → Verwalten → Eigenschaften anzeigen → Erweiterte Startoptionen.',
  'gameDetails.luma.launchArgs.instructions.ubisoft':
    'Wenn Sie das Spiel über Ubisoft Connect starten, fügen Sie sie dort hinzu: Spiel auswählen → Eigenschaften → Startargumente hinzufügen.',
  'gameDetails.luma.launchArgs.instructions.other':
    'Verwenden Sie den Startweg, der das Spiel tatsächlich öffnet. Fügen Sie die Argumente im Launcher, im Ziel der Verknüpfung, in einer Batchdatei oder einem anderen Loader hinzu.',
  'gameDetails.luma.launchArgs.title': 'Startargumente erforderlich',
  'gameDetails.luma.launchArgs.dx11Title': 'Dieses Luma-Profil benötigt DirectX 11',
  'gameDetails.luma.launchArgs.copyStep': 'Kopieren Sie die erforderlichen Startargumente:',
  'gameDetails.luma.launchArgs.copy': 'Argumente kopieren',
  'gameDetails.luma.launchArgs.copied': 'Kopiert',
  'gameDetails.luma.launchArgs.copyFailed': 'Startargumente konnten nicht kopiert werden',
  // ── Game details: Luma attribution ──
  'gameDetails.luma.attribution': 'Luma Framework von Filoppi.',
  'gameDetails.luma.attributionLink': 'Projekt ansehen',
  'gameDetails.luma.guidance.gameSetting': 'Spieleinstellung',
  'gameDetails.luma.guidance.engineIni': 'Manuelle INI-Änderung',
  'gameDetails.luma.guidance.launchArgument': 'Startargument',
  'gameDetails.luma.guidance.warning': 'Wichtig',
  'gameDetails.luma.guidance.compatibility': 'Kompatibilitätshinweis',
  'gameDetails.luma.guidance.externalTool': 'Drittanbieterwerkzeug',
  'gameDetails.luma.guidance.copy': 'Kopieren',
  'gameDetails.luma.guidance.copied': 'Kopiert',
  'gameDetails.luma.guidance.copyFailed': 'Kopieren fehlgeschlagen',
});
