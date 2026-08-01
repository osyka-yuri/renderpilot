import type { EnglishCatalog } from './en';
import { defineLocalizedCatalog } from './contract';
import { plural } from './model';

export const fr = defineLocalizedCatalog<'fr', EnglishCatalog>()({
  'nav.games': 'Jeux',
  'nav.libraries': 'Bibliothèques',
  'nav.settings': 'Paramètres',
  'nav.operations': 'Journal',
  'nav.gameFallback': 'Jeu',
  'nav.donate': 'Faire un don',
  'shell.refresh': 'Actualiser',
  'shell.updateAvailable': 'Mise à jour disponible',

  'settings.appearance.title': 'Apparence',
  'settings.appearance.description': 'Personnalisez l’apparence de l’application et la langue.',
  'settings.appearance.theme.title': 'Thème',
  'settings.appearance.theme.description': 'Choisissez un thème de couleurs pour l’application.',
  'settings.appearance.theme.triggerLabel': 'Thème',
  'settings.appearance.language.title': 'Langue',
  'settings.appearance.language.description': 'Sélectionnez la langue de l’interface.',
  'settings.appearance.language.triggerLabel': 'Langue',
  'settings.appearance.language.placeholder': 'Sélectionner une langue',

  'settings.theme.system': 'Système',
  'settings.theme.dark': 'Sombre',
  'settings.theme.light': 'Clair',

  'settings.language.system': 'Par défaut du système',
  'settings.language.en': 'English',
  'settings.language.ru': 'Русский',
  'settings.language.es': 'Español',
  'settings.language.zh': '中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  'settings.tabs.general': 'Général',
  'settings.tabs.renodx': 'RenoDX',
  'settings.tabs.catalog': 'Catalogue',
  'settings.tabs.nvidia': 'NVIDIA',

  // ── Settings: NVIDIA ──
  'settings.nvidia.indicator.title': 'Indicateur DLSS',
  'settings.nvidia.indicator.description':
    'Affiche une superposition avec la version et les paramètres DLSS actifs pendant le jeu.',
  'settings.nvidia.indicator.systemWide': 'Tout le système',
  'settings.nvidia.indicator.adminRequired':
    'Redémarrez l’application en tant qu’administrateur pour modifier ce paramètre.',
  'settings.nvidia.indicator.overlayTitle': 'Superposition à l’écran',
  'settings.nvidia.indicator.overlayDescription': 'S’applique à tous les jeux de ce PC.',
  'settings.nvidia.indicator.toggleAria': 'Basculer l’indicateur DLSS',
  'settings.nvidia.global.title': 'Paramètres DLSS globaux',
  'settings.nvidia.global.description':
    'Valeurs par défaut appliquées à chaque jeu sans réglage spécifique, via le profil de base NVIDIA.',
  'settings.nvidia.global.systemWide': 'À l’échelle du système',
  'settings.nvidia.global.adminRequired':
    'Redémarrez l’application en tant qu’administrateur pour modifier ces paramètres.',
  'settings.nvidia.global.familySr': 'DLSS Super Resolution',
  'settings.nvidia.global.familyFg': 'DLSS Frame Generation',
  'settings.nvidia.global.familyRr': 'DLSS Ray Reconstruction',
  'settings.nvidia.unsupported.title': 'Aucun GPU NVIDIA détecté',
  'settings.nvidia.unsupported.description':
    'Ces paramètres nécessitent une carte graphique NVIDIA prise en charge.',

  'game.card.action.details': 'Détails',
  'game.card.action.detailsAria': 'Ouvrir les détails pour {title}',
  'game.card.detectedLibraries': 'Composants détectés',
  'game.card.availableAddons': 'Add-ons disponibles',
  'game.card.badge.upToDate': 'À jour',
  'game.card.badge.updatesAvailable': 'Mises à jour disponibles',
  'game.card.badge.updatesAvailableCount': plural('count', {
    one: '1 mise à jour disponible',
    many: '{count} mises à jour disponibles',
    other: '{count} mises à jour disponibles',
  }),
  'game.card.menu.ariaLabel': 'Options pour {title}',
  'game.card.menu.favorite.add': 'Ajouter aux favoris',
  'game.card.menu.favorite.remove': 'Retirer des favoris',
  'game.card.menu.favorite.toggleHint': "Basculer l'état favori pour ce jeu.",
  'game.card.menu.hidden.add': 'Masquer le jeu',
  'game.card.menu.hidden.remove': 'Afficher le jeu',
  'game.card.menu.hidden.toggleHint': "Basculer l'état masqué pour ce jeu.",
  'game.card.menu.removeFromCatalog': 'Retirer du catalogue',
  'game.card.menu.removeFromCatalogHint': 'Oublier ce jeu ajouté manuellement.',
  'game.card.removeConfirm.title': 'Retirer {title} du catalogue ?',
  'game.card.removeConfirm.description':
    'RenderPilot annulera en toute sécurité les modifications gérées, puis retirera la carte et son historique. Les fichiers du jeu ne seront pas modifiés.',
  'game.card.removeConfirm.action': 'Retirer du catalogue',

  'game.cover.alt': 'Jaquette',
  'game.cover.altWithTitle': 'Jaquette : {title}',
  'game.cover.menu.fetch': 'Télécharger la jaquette',
  'game.cover.menu.fetching': 'Téléchargement…',
  'game.cover.menu.fetchHint': 'Rechercher une jaquette en ligne.',
  'game.cover.menu.pick': 'Choisir un fichier image…',
  'game.cover.menu.pickHint': 'Sélectionnez une image locale à utiliser comme jaquette.',
  'game.cover.menu.clear': 'Supprimer la jaquette',
  'game.cover.menu.clearHint': 'Restaurer la jaquette par défaut.',

  'game.dashboard.summary': 'Tableau de bord',
  'game.dashboard.games': plural('count', {
    one: '{count} jeu',
    many: '{count} jeux',
    other: '{count} jeux',
  }),
  'game.dashboard.updates': plural('count', {
    one: '{count} mise à jour',
    many: '{count} mises à jour',
    other: '{count} mises à jour',
  }),

  'elevation.title': 'Privilèges d’administrateur requis',
  'elevation.description':
    'Certains paramètres nécessitent des droits d’administrateur pour être modifiés.',
  'elevation.relaunch': 'Redémarrer en tant qu’administrateur',
  'elevation.relaunchFailed': 'Impossible de redémarrer en tant qu’administrateur',
  'elevation.dismiss': 'Ignorer',
  'error.boundary.title': 'Une erreur est survenue',
  'error.boundary.description':
    'Cet écran a rencontré une erreur inattendue. Réessayez ou passez à une autre section.',
  'error.boundary.reset': 'Réessayer',
  'pageLoad.loading': 'Chargement de la page…',
  'pageLoad.error.title': "Impossible d'ouvrir cette page",
  'pageLoad.error.description': "La page n'a pas pu être chargée. Réessayez ou revenez aux jeux.",
  'pageLoad.error.retry': 'Réessayer',
  'pageLoad.error.backToGames': 'Retour aux jeux',

  'games.addGame': 'Ajouter un jeu',
  'games.addingGame': 'Ajout du jeu...',
  'games.chooseInstallFolder': 'Choisir le dossier d’installation du jeu',
  'addGame.title': 'Ajouter un jeu',
  'addGame.cannotAddTitle': 'Impossible d’ajouter le jeu',
  'addGame.installRoot': 'Racine d’installation',
  'addGame.reviewTitle': 'Vérifier l’installation du jeu',
  'addGame.reviewDescription': 'Confirmez la racine d’installation avant d’ajouter un jeu.',
  'addGame.selectedFolder': 'Dossier sélectionné',
  'addGame.recommendedFolder': 'Racine d’installation recommandée',
  'addGame.existingRoot': 'Dossier actuel du jeu',
  'addGame.chooseExecutable': 'Exécutable du jeu',
  'addGame.chooseExecutablePlaceholder': 'Choisir un exécutable',
  'addGame.chooseAnother': 'En choisir un autre',
  'addGame.add': 'Ajouter le jeu',
  'addGame.addSelected': 'Ajouter le dossier sélectionné',
  'addGame.correctRoot': 'Corriger le chemin',
  'addGame.addRecommended': 'Ajouter la racine recommandée',
  'addGame.replaceRootTitle': 'Corriger le chemin du jeu',
  'addGame.replaceRootDescription':
    'RenderPilot utilisera le dossier sélectionné à la place du dossier actuel. Les fichiers du jeu resteront inchangés.',
  'addGame.replaceExistingRoot': 'Corriger le chemin',
  'addGame.rootCorrection.rollbackTitle':
    'Les modifications actives des composants doivent d’abord être annulées',
  'addGame.rootCorrection.rollbackDescription': plural('count', {
    one: 'RenderPilot doit annuler la modification active d’un composant avant de remplacer la racine de la fiche.',
    many: 'RenderPilot doit annuler les modifications actives de {count} composants avant de remplacer la racine de la fiche.',
    other:
      'RenderPilot doit annuler les modifications actives de {count} composants avant de remplacer la racine de la fiche.',
  }),
  'addGame.rootCorrection.rollbackAndReplace': 'Annuler les modifications et remplacer la racine',
  'addGame.rootCorrection.rollbackFailed':
    'Les modifications des composants n’ont pas pu être entièrement annulées. La racine actuelle du jeu n’a pas été modifiée.',
  'addGame.rootCorrection.blocker.pendingRecovery':
    'Une opération de fichiers interrompue doit encore être récupérée.',
  'addGame.rootCorrection.blocker.installedAddon':
    'Un module installé dépend de fichiers situés hors du dossier sélectionné.',
  'addGame.rootCorrection.blocker.nvapi':
    'Des paramètres de profil NVIDIA actifs concernent des exécutables hors du dossier sélectionné.',
  'addGame.rootCorrection.blocker.orphanedComponentBaseline':
    'Un état de restauration enregistré ne correspond plus à aucun composant.',
  'addGame.rescan': 'Analyser à nouveau le jeu',
  'addGame.catalogBusy':
    'Une autre opération sur le catalogue est en cours. Terminez-la puis réessayez.',
  'addGame.warning.legacyCardsConsolidated': plural('count', {
    one: 'Une ancienne fiche de jeu, confirmée comme erronée, a été fusionnée.',
    many: '{count} anciennes fiches de jeu, confirmées comme erronées, ont été fusionnées.',
    other: '{count} anciennes fiches de jeu, confirmées comme erronées, ont été fusionnées.',
  }),
  'addGame.warning.legacyCardsRetained': plural('count', {
    one: 'Une ancienne fiche a été conservée, faute de preuve concluante d’une installation indépendante.',
    many: '{count} anciennes fiches ont été conservées, faute de preuves concluantes d’installations indépendantes.',
    other:
      '{count} anciennes fiches ont été conservées, faute de preuves concluantes d’installations indépendantes.',
  }),
  'addGame.warning.recoveryBundleCreated':
    'L’ancien état en conflit a été conservé dans le paquet de récupération {path}.',
  'addGame.warning.rootCorrectionHistoryArchived':
    'L’historique du catalogue hors de la racine corrigée a été conservé dans le paquet de récupération {path}.',
  'addGame.warning.recoveryBundleFallback': 'Paquet de récupération : {path}',
  'addGame.warning.unsupportedPlatform':
    'L’inspection des installations de jeux est uniquement prise en charge sous Windows.',
  'addGame.warning.probeIncomplete':
    'Certains dossiers n’ont pas pu être inspectés. La recommandation est donc moins fiable.',
  'addGame.warning.parentProbeIncomplete':
    'Le dossier parent recommandé n’a pas pu être inspecté complètement. Vérifiez-le avant de l’ajouter.',
  'addGame.unavailable.multipleInstalls':
    'Le dossier sélectionné semble être une bibliothèque commune contenant plusieurs jeux. Sélectionnez le dossier d’un jeu précis.',
  'addGame.unavailable.containsProvenInstall':
    'Une installation de jeu déjà reconnue se trouve dans le dossier sélectionné. Sélectionnez le dossier exact de ce jeu plutôt que le dossier parent commun.',
  'addGame.unavailable.containsMultipleCatalogInstalls':
    'Plusieurs jeux déjà reconnus se trouvent dans le dossier sélectionné. Sélectionnez le dossier d’un jeu précis.',
  'addGame.unavailable.insideExistingInstall':
    'Le dossier sélectionné se trouve dans un jeu déjà ajouté. Utilisez la racine d’installation de ce jeu.',
  'addGame.unavailable.noReadableExecutable':
    'Aucun exécutable de jeu lisible n’a été trouvé dans le dossier sélectionné. Sélectionnez le dossier d’installation qui contient l’exécutable du jeu.',
  'addGame.unavailable.rootCorrectionBlocked':
    'La racine d’installation existante ne peut pas être modifiée en toute sécurité tant qu’un état géré est présent. Résolvez d’abord les blocages indiqués.',
  'addGame.warning.insideExistingInstall':
    'Ce dossier appartient à un jeu existant. Utilisez la racine de son installation.',
  'addGame.warning.narrowsExistingInstall':
    'La racine manuelle existante semble contenir plusieurs dossiers de jeux. La confirmation conservera la même carte, mais corrigera sa racine vers le dossier sélectionné.',
  'addGame.warning.multipleProvenInstalls':
    'Ce dossier contient plusieurs installations de jeux confirmées.',
  'addGame.warning.containsProvenInstall':
    'Ce dossier contient une installation de jeu confirmée. Utilisez sa racine exacte.',
  'addGame.warning.multipleInstallsSuspected':
    'Les exécutables de différents sous-dossiers peuvent appartenir à plusieurs jeux. En cas de confirmation, ce dossier sera néanmoins traité comme un seul jeu.',
  'addGame.warning.explicitExecutableRequired':
    'Tous les exécutables valides ressemblent à des lanceurs ou à des utilitaires. Sélectionnez-en un explicitement.',
  'addGame.warning.noReadableExecutable':
    'Ce dossier ne peut pas être ajouté séparément, car il ne contient aucun exécutable de jeu lisible.',
  'addGame.warning.filesystemProbeError':
    'Une partie de l’installation n’a pas pu être inspectée. Vérifiez les autorisations d’accès aux fichiers.',
  'games.libraryActions': 'Actions',
  'games.search': 'Rechercher des jeux',
  'games.openFilters': 'Filtres',
  'games.openFiltersActive': 'Filtres (actifs)',
  'games.loading': 'Chargement...',
  'games.empty.title': 'Aucun jeu trouvé',
  'games.empty.description': 'Ajoutez un jeu pour l’afficher dans le tableau de bord.',
  'games.filterEmpty.title': 'Aucun résultat',
  'games.filterEmpty.description': 'Essayez de modifier votre recherche ou vos filtres.',
  'games.filterEmpty.reset': 'Réinitialiser les filtres',

  'settings.catalog.title': 'Sources de jaquettes',
  'settings.catalog.description':
    'Sélectionnez des sources en ligne pour télécharger les jaquettes.',
  'settings.catalog.steamKey.srLabel': 'Clé API SteamGridDB',
  'settings.catalog.steamKey.placeholder': 'Clé API',
  'settings.catalog.steamKey.loading': 'Chargement…',
  'settings.catalog.steamKey.save': 'Enregistrer',
  'settings.catalog.steamKey.saved': 'Enregistré',
  'settings.catalog.steamKey.cleared': 'Effacé',
  'settings.catalog.steamKey.readError': 'Échec de la lecture des paramètres.',
  'settings.catalog.steamKey.saveError': 'Échec de l’enregistrement des paramètres.',
  'settings.catalog.steamKey.show': 'Afficher la clé API',
  'settings.catalog.steamKey.hide': 'Masquer la clé API',
  'settings.catalog.steamKey.getKey': 'Obtenir une clé API',

  'settings.renodx.vulkan.description':
    'Gérer la couche Vulkan ReShade partagée utilisée par les jeux Vulkan RenoDX.',
  'settings.renodx.vulkan.channel': 'Canal de la couche Vulkan',
  'settings.renodx.vulkan.channelDescription':
    'Choisissez le canal ReShade utilisé par la couche Vulkan partagée.',
  'settings.renodx.vulkan.loadError': "Impossible de charger l'état de la couche Vulkan.",
  'settings.renodx.vulkan.saveError': 'Impossible d’enregistrer le canal de la couche Vulkan.',
  'settings.renodx.vulkan.applyError': 'Impossible d’appliquer la couche Vulkan.',

  'common.unknown': 'Inconnu',
  'common.downloadProgress': 'Progression du téléchargement',

  'gameDetails.noGameSelected.title': 'Aucun jeu sélectionné',
  'gameDetails.noGameSelected.description':
    'Sélectionnez un jeu dans le tableau de bord pour voir ses détails.',

  'gameDetails.version.noReplacements': 'Aucune version alternative',
  'gameDetails.version.restoreOriginal': 'Restaurer {fileName} original',
  'gameDetails.version.fileCount': plural('count', {
    one: '1 fichier',
    many: '{count} fichiers',
    other: '{count} fichiers',
  }),

  'gameDetails.vendor.description': 'Modifier la version du composant.',

  'gameDetails.dlss.description': 'Modifier la version de DLSS ou écraser ses paramètres.',
  'gameDetails.dlss.descriptionSwapOnly': 'Modifier la version de DLSS.',
  'gameDetails.dlss.libraryFileLabel': 'Version du fichier',
  'gameDetails.dlss.driverOverridesLabel': 'Remplacements de profil NVIDIA',
  'gameDetails.dlss.adminRequired':
    'Redémarrez l’application en tant qu’administrateur pour modifier ces paramètres.',

  'gameDetails.streamline.description': 'Gérer les plugins Streamline.',
  'gameDetails.streamline.versionTitle': 'Version globale de Streamline',
  'gameDetails.streamline.versionDescription': 'Applique la même version à tous les plugins.',
  'gameDetails.streamline.noOtherVersions': 'Aucune autre version',
  'gameDetails.streamline.mixed': 'Versions mixtes',
  'gameDetails.streamline.mixedRange': 'Versions mixtes (v{min} – v{max})',
  'gameDetails.streamline.updatesSummary': '{updates} mises à jour · {missing} manquants',
  'gameDetails.streamline.restoreAllAria': 'Restaurer tous les plugins à l’original',
  'gameDetails.streamline.restoreAllTooltip': 'Tout restaurer à l’original',
  'gameDetails.updateAll.action': 'Tout mettre à jour',
  'gameDetails.updateAll.actionCount': 'Tout mettre à jour ({count})',
  'gameDetails.updateAll.upToDate': 'Toutes les versions stables sont à jour',
  'gameDetails.updateAll.partialFailure':
    'Certaines mises à jour ont échoué ({count}). Vérifiez les détails et réessayez.',
  'gameDetails.updateAll.tooltip': plural('count', {
    one: 'Mettre à jour 1 composant vers sa dernière version stable',
    many: 'Mettre à jour {count} composants vers leur dernière version stable',
    other: 'Mettre à jour {count} composants vers leur dernière version stable',
  }),
  'gameDetails.executable.title': 'Exécutable du jeu',
  'gameDetails.developerMode.requiredTitle': 'Le mode développeur Windows est désactivé',
  'gameDetails.developerMode.requiredDescription':
    'Microsoft D3D12 Agility Preview nécessite ce paramètre Windows.',
  'gameDetails.developerMode.checkTitle': 'Impossible de vérifier le mode développeur',
  'gameDetails.developerMode.checkDescription':
    'RenderPilot n’a pas pu déterminer l’état actuel du mode développeur Windows.',
  'gameDetails.developerMode.checkUnavailable':
    'Une vérification réussie est nécessaire avant de continuer.',
  'gameDetails.developerMode.enableGuidance':
    'Le mode développeur peut être activé sous « Espace développeurs » dans les paramètres Windows.',
  'gameDetails.developerMode.previewGuidance':
    'La documentation Microsoft explique comment activer le mode développeur dans Windows.',
  'gameDetails.developerMode.restartInfo':
    'Dans certains cas, Windows applique ce paramètre uniquement après un redémarrage.',
  'gameDetails.developerMode.stillDisabled':
    'Le mode développeur est toujours désactivé. S’il vient d’être activé, un redémarrage de Windows peut être nécessaire pour appliquer la modification.',
  'gameDetails.developerMode.settingsOpenFailed':
    'Impossible d’ouvrir les paramètres Windows. Ouvrez « Espace développeurs » manuellement.',
  'gameDetails.developerMode.documentationOpenFailed':
    'Impossible d’ouvrir la documentation Microsoft.',
  'gameDetails.developerMode.openSettings': 'Ouvrir les paramètres',
  'gameDetails.developerMode.openDocumentation': 'Ouvrir la documentation',
  'gameDetails.developerMode.checkStatus': 'Vérifier l’état',
  'gameDetails.developerMode.retryCheck': 'Relancer la vérification',
  'gameDetails.developerMode.checkingStatus': 'Vérification…',
  'gameDetails.d3d12.status.original': 'EXE original',
  'gameDetails.d3d12.status.patched': 'EXE patché : {from} → {to}',
  'gameDetails.d3d12.status.repair': 'Réparation requise',
  'gameDetails.d3d12.repairGuidance':
    'Vérifiez les fichiers du jeu puis relancez l’analyse. RenderPilot ne remplacera pas cet EXE.',
  'gameDetails.d3d12.action.patch': 'Patcher l’EXE : {from} → {to}',
  'gameDetails.d3d12.action.restore': 'Restaurer l’EXE : {from} → {to}',
  'gameDetails.d3d12.action.repair': 'L’EXE doit d’abord être réparé',
  'gameDetails.d3d12.action.blocked':
    'Cette version de D3D12 ne peut pas être appliquée dans l’état actuel.',
  'gameDetails.d3d12.action.planPatch': 'Un patch sera appliqué : SDK {from} → {to}',
  'gameDetails.d3d12.action.planRestore': 'L’EXE original sera restauré : SDK {from} → {to}',
  'gameDetails.d3d12.select.compatible': 'Compatible avec l’EXE actuel',
  'gameDetails.d3d12.select.changesExecutable': 'Nécessite une modification de l’EXE',
  'gameDetails.d3d12.select.unavailable': 'Indisponible',
  'gameDetails.d3d12.confirm.title': 'Confirmer la modification de l’EXE',
  'gameDetails.d3d12.confirm.description':
    'RenderPilot modifiera l’export D3D12SDKVersion de l’exécutable.',
  'gameDetails.d3d12.confirm.updateAllDescription':
    'Ces mises à jour exigent que les exécutables indiqués changent de ligne SDK D3D12. Aucun téléchargement ni changement n’aura lieu avant confirmation.',
  'gameDetails.d3d12.confirm.backup': 'Chemin de sauvegarde : {path}',
  'gameDetails.d3d12.confirm.backupWillCreate':
    'Avant la modification, une copie de sauvegarde de l’EXE original sera créée ici : {path}',
  'gameDetails.d3d12.confirm.backupExists':
    'L’EXE original est déjà enregistré ici : {path}. Cette copie ne sera pas écrasée.',
  'gameDetails.d3d12.confirm.signatureWarning':
    'Après la modification, la signature numérique de l’EXE peut être considérée comme non valide et les contrôles d’intégrité peuvent signaler que le fichier a été modifié. Lors d’une restauration complète de D3D12, RenderPilot restaure l’EXE original.',
  'gameDetails.d3d12.confirm.accept': 'Modifier',
  'gameDetails.d3d12.executableLockedTitle': 'Sélection de l’EXE verrouillée',
  'gameDetails.d3d12.executableLocked':
    'Pour choisir un autre EXE, restaurez entièrement le composant D3D12.',
  'gameDetails.d3d12.executableRepairLocked':
    'Suivez les étapes de récupération indiquées dans la carte D3D12, puis analysez de nouveau le jeu.',
  'gameDetails.executable.description':
    'L’exécutable du jeu : le profil NVIDIA s’y applique et RenoDX s’installe dans son dossier.',
  'gameDetails.executable.triggerAria': 'Exécutable du jeu : {fileName}',
  'gameDetails.executable.detectedGroup': 'Exécutables de jeu détectés',
  'gameDetails.executable.otherGroup': 'Autres (lanceurs, installateurs, outils)',
  'gameDetails.executable.customBadge': 'Manuel',
  'gameDetails.executable.reset': 'Réinitialiser sur la détection automatique',
  'gameDetails.executable.tooltipAuto':
    'Exécutable du jeu : détecté automatiquement. Utilisé par le profil NVIDIA et RenoDX.',
  'gameDetails.executable.tooltipCustom':
    'Exécutable du jeu : sélectionné manuellement. Utilisé par le profil NVIDIA et RenoDX.',
  'gameDetails.profile.title': 'Profil NVIDIA',
  'gameDetails.profile.description': 'Configurer les paramètres du pilote NVIDIA pour ce jeu.',
  'gameDetails.profile.pinnedManual': 'Sélectionné manuellement.',
  'gameDetails.profile.autoDetected': 'Détecté automatiquement.',
  'gameDetails.profile.noExeDetected': 'Aucun exécutable trouvé pour ce jeu.',
  'gameDetails.profile.noExe': 'Aucun exécutable',
  'gameDetails.profile.noProfile': 'Profil NVIDIA introuvable.',

  'gameDetails.nvapi.requiresDriver': 'nécessite le pilote {version}+',
  'gameDetails.nvapi.unavailable': 'indisponible',
  'gameDetails.nvapi.resetDefault': 'Réinitialiser par défaut',
  'gameDetails.nvapi.alreadyDefault': 'Déjà par défaut',
  'gameDetails.nvapi.restoreBaselineAria': 'Restaurer la valeur initiale',
  'gameDetails.nvapi.restoreBaseline': 'Restaurer la valeur initiale',
  'gameDetails.nvapi.alreadyBaseline': 'Déjà à la valeur initiale',
  'gameDetails.nvapi.noBaseline': 'Aucune valeur initiale enregistrée',

  'gameDetails.nvapi.warning.noDll': "Aucune DLL DLSS détectée dans le répertoire d'installation.",
  'gameDetails.nvapi.warning.noManifest':
    "Le manifeste n'a aucune entrée pour cette version de DLL.",
  'gameDetails.nvapi.warning.noExecutable': 'Aucun exécutable trouvé pour ce jeu.',
  'gameDetails.nvapi.warning.nvapiUnavailable': 'NVAPI indisponible.',
  'gameDetails.nvapi.warning.nvapiInitFailed': "Échec de l'initialisation de NVAPI.",
  'gameDetails.nvapi.warning.drsFailed': 'Impossible de créer la session DRS.',

  'operations.title': 'Historique',
  'operations.subtitleGame': 'Activité pour {title}',
  'operations.loading': 'Chargement...',
  'operations.empty': 'Aucun historique',
  'operations.gameName': 'Jeu',
  'operations.date': 'Date',
  'operations.status': 'Statut',
  'operations.action': 'Action',
  'operations.libraryType': 'Type de bibliothèque',
  'operations.version': 'Version',

  'libraries.error': 'Erreur',
  'libraries.catalogFallback.title': 'Catalogue indisponible',
  'libraries.catalogFallback.description':
    "Seuls les paquets enregistrés localement sont affichés. Il ne s'agit pas du catalogue complet.",
  'libraries.state.localOnly': 'Local uniquement',
  'libraries.state.downloaded': 'Téléchargé',
  'libraries.state.missing': 'Fichiers manquants',
  'libraries.state.corrupt': 'Fichiers corrompus',
  'libraries.hash.copy': 'Copier le hash',
  'libraries.hash.copied': 'Copié',
  'libraries.hash.failed': 'Échec de la copie',
  'libraries.hash.copiedToast': 'Hash copié dans le presse-papiers',
  'libraries.sort.asc': 'Ordre croissant',
  'libraries.sort.desc': 'Ordre décroissant',
  'libraries.sort.none': 'Non trié',
  'libraries.actions.delete': 'Supprimer',
  'libraries.actions.download': 'Télécharger',
  'libraries.actions.deletedToast': 'Supprimé {version}',
  'libraries.actions.downloadedToast': 'Téléchargé {version}',
  'libraries.actions.failedToast': 'Échec : {action}',
  'libraries.actions.downloadAll': 'Télécharger les dernières',
  'libraries.actions.downloadAllCount': 'Télécharger les dernières ({count})',
  'libraries.actions.downloadAllUpToDate': 'Toutes les dernières versions sont déjà téléchargées',
  'libraries.actions.downloadAllTooltip': plural('count', {
    one: 'Télécharger 1 dernière version',
    many: 'Télécharger {count} dernières versions',
    other: 'Télécharger {count} dernières versions',
  }),
  'libraries.actions.downloadAllDoneToast': plural('count', {
    one: '{count} bibliothèque téléchargée',
    many: '{count} bibliothèques téléchargées',
    other: '{count} bibliothèques téléchargées',
  }),
  'libraries.actions.downloadAllPartialToast': '{succeeded} téléchargées, {failed} en échec',
  'libraries.actions.downloadAllNoneToast': 'Toutes les dernières versions sont déjà téléchargées',

  'common.cancel': 'Annuler',
  'common.apply': 'Appliquer',

  'filters.title': 'Filtres',
  'filters.launchers.title': 'Lanceurs',
  'filters.launchers.empty': 'Aucun lanceur trouvé',
  'filters.launchers.reorder': 'Déplacer {label}',
  'filters.libraries.title': 'Composants',
  'filters.libraries.empty': 'Aucun composant trouvé',
  'filters.addons.title': 'Add-ons',

  'games.favoritesToggle': 'Favoris',
  'games.favoritesToggleActive': 'Favoris (actifs)',
  'games.showHiddenActive': 'Jeux masqués (actifs)',
  'games.showHidden': 'Afficher',

  'operation.label.low': 'Risque faible',
  'operation.label.medium': 'Risque moyen',
  'operation.label.high': 'Risque élevé',
  'operation.label.blocked': 'Bloqué',
  'operation.label.planned': 'Planifié',
  'operation.label.completed': 'Terminé',
  'operation.label.failed': 'Échec',
  'operation.label.rolledBack': 'Restauré',
  'operation.label.replaceComponent': 'Modifier la version',
  'operation.duration': 'Terminé en {seconds}s',
  'operation.filesUpdated.none': 'Aucun fichier mis à jour.',
  'operation.filesUpdated.count': plural('count', {
    one: '1 fichier mis à jour.',
    many: '{count} fichiers mis à jour.',
    other: '{count} fichiers mis à jour.',
  }),
  'operation.filesRestored.none': 'Aucun fichier restauré.',
  'operation.filesRestored.count': plural('count', {
    one: '1 fichier restauré.',
    many: '{count} fichiers restaurés.',
    other: '{count} fichiers restaurés.',
  }),
  'operation.itemAria': '{kind}, {status}',

  'notify.stalePlan': 'Le plan d’opération est obsolète. Veuillez réessayer.',
  'notify.missingStableGameId': 'Impossible d’identifier le jeu.',
  'notify.coverPickerPreview':
    'Veuillez utiliser l’application de bureau pour choisir une jaquette.',
  'notify.coverUpdated.title': 'Jaquette mise à jour',
  'notify.coverUpdated.body': 'Votre jaquette personnalisée a été enregistrée.',
  'notify.coverDownloaded.title': 'Jaquette téléchargée',
  'notify.coverDownloaded.body': 'La jaquette du jeu a été mise à jour.',
  'notify.coverRemoved.title': 'Jaquette supprimée',
  'notify.coverRemoved.body': 'Jaquette par défaut restaurée.',
  'notify.favoriteFailed': "Impossible de modifier l'état des favoris.",
  'notify.favoriteAdded': 'Ajouté aux favoris.',
  'notify.favoriteRemoved': 'Retiré des favoris.',
  'notify.hiddenFailed': "Impossible de modifier l'état masqué.",
  'notify.gameHidden': 'Jeu masqué.',
  'notify.gameUnhidden': 'Jeu affiché.',
  'notify.gameRemovedFromCatalog': 'Jeu retiré du catalogue.',
  'notify.removeGameFailed': 'Impossible de retirer le jeu du catalogue.',
  'notify.applyCompleted': 'Modifications appliquées',
  'notify.rollbackCompleted': 'Restauration terminée',
  'notify.swapBatchFailed.title': 'Certaines mises à jour ont échoué',
  'notify.swapBatchFailed.description':
    'Échec de la mise à jour de {failed} composants sur {total}.',
  'notify.rollbackBatchFailed.title': 'Certaines restaurations ont échoué',
  'notify.rollbackBatchFailed.description':
    'Échec de la restauration de {failed} composants sur {total}.',
  'notify.statusError': 'Erreur',
  'notify.statusWarning': 'Avertissement',

  'scan.partialWarning': plural('count', {
    one: 'Impossible d’analyser 1 dossier.',
    many: 'Impossible d’analyser {count} dossiers.',
    other: 'Impossible d’analyser {count} dossiers.',
  }),

  'coverSync.failed': 'Échec de la synchronisation des jaquettes.',
  'coverSync.refreshFailed': 'Échec de la synchronisation des jaquettes.',

  'nvidia.adminRequired': 'Privilèges d’administrateur requis',
  'nvidia.relaunchTo': 'Redémarrez en tant qu’administrateur pour {action}.',
  'nvidia.action.changeSetting': 'appliquer les paramètres',
  'nvidia.action.revertSetting': 'rétablir les paramètres',
  'nvidia.changeSettingFailed': 'Échec de l’application des paramètres',
  'nvidia.revertDefaultFailed': 'Échec de la restauration des paramètres par défaut',
  'nvidia.revertBaselineFailed': 'Échec de la restauration des paramètres initiaux',

  'indicator.relaunchToToggle':
    'Redémarrez en tant qu’administrateur pour basculer l’indicateur DLSS.',
  'indicator.changeFailed': 'Échec de la bascule de l’indicateur DLSS',

  'libraries.column.version': 'Version',
  'libraries.column.hash': 'Hash',
  'libraries.column.signed': 'Signé',
  'libraries.column.size': 'Taille',
  'libraries.column.documents': 'Documents',
  'libraries.column.actions': 'Actions',
  'libraries.documents.openForVersion': 'Ouvrir les documents juridiques de {name} {version}',
  'libraries.documents.title': 'Documents juridiques',
  'libraries.documents.description': 'S’appliquent à {name} {version}.',
  'libraries.documents.formatPdf': 'PDF',
  'libraries.documents.formatText': 'Texte',
  'libraries.documents.open': 'Ouvrir',
  'libraries.documents.openFailed': 'Impossible d’ouvrir le document',
  'libraries.unsigned': 'Non signé',
  'libraries.invalidDate': 'Date non valide',
  'libraries.empty.loading': 'Chargement…',
  'libraries.empty.unavailable': 'Impossible de charger les bibliothèques',
  'libraries.empty.none': 'Aucune bibliothèque trouvée',
  'libraries.error.loadFailed': 'Impossible de charger les bibliothèques',
  'libraries.error.refreshFailed': 'Impossible d’actualiser le manifeste',
  'libraries.error.downloadFailed': 'Échec du téléchargement',
  'libraries.error.deleteFailed': 'Échec de la suppression',
  'libraries.error.downloadedRefreshFailed':
    'Bibliothèque téléchargée, mais l’actualisation du statut a échoué',
  'libraries.error.deletedRefreshFailed':
    'Bibliothèque supprimée, mais l’actualisation du statut a échoué',

  'settings.catalog.source.steam.aria': 'Télécharger les jaquettes depuis Steam',
  'settings.catalog.source.steam.title': 'Steam',
  'settings.catalog.source.steam.description':
    'Télécharger les jaquettes du catalogue public de Steam.',
  'settings.catalog.source.gog.aria': 'Télécharger les jaquettes depuis GOG',
  'settings.catalog.source.gog.title': 'GOG',
  'settings.catalog.source.gog.description':
    'Télécharger les jaquettes du catalogue officiel de GOG.',
  'settings.catalog.source.steamgriddb.aria': 'Télécharger les jaquettes depuis SteamGridDB',
  'settings.catalog.source.steamgriddb.title': 'SteamGridDB',
  'settings.catalog.source.steamgriddb.description':
    'Télécharger les jaquettes communautaires depuis SteamGridDB. Nécessite une clé API.',
  'settings.catalog.artworkReadError': 'Échec du chargement des paramètres de jaquette.',
  'settings.catalog.artworkSaveError': 'Échec de l’enregistrement des paramètres de jaquette.',

  'user_message.invalid_argument': 'Entrée fournie invalide.',
  'user_message.invalid_install_root':
    'Choisissez le dossier d’installation d’un seul jeu. Les racines de lecteur, de partage réseau et les dossiers système ne peuvent pas être ajoutés.',
  'user_message.multiple_installs_detected':
    'Ce dossier contient plusieurs installations de jeux. Sélectionnez le dossier d’installation d’un seul jeu.',
  'user_message.stale_install_inspection':
    'L’installation a changé pendant la vérification. Examinez le résultat actualisé avant de l’ajouter.',
  'user_message.root_correction_cleanup_required':
    'Les modifications actives des composants doivent être annulées avant de changer la racine du jeu.',
  'user_message.root_correction_blocked':
    'Résolvez l’état actif depuis la fiche existante avant de changer la racine du jeu.',
  'user_message.managed_cleanup_ambiguous':
    'RenderPilot a trouvé des modifications gérées qui se chevauchent sans ordre de restauration sûr démontrable. Rien n’a été modifié et un paquet de récupération a été créé.',
  'user_message.game_removal_cleanup_failed':
    'RenderPilot n’a pas pu restaurer les fichiers d’origine du jeu. La carte n’a donc pas été retirée. Vérifiez les fichiers du jeu et réessayez.',
  'user_message.invalid_game_reference': 'Jeu introuvable.',
  'user_message.invalid_component_reference': 'Composant introuvable.',
  'user_message.invalid_artifact_reference': 'Élément introuvable.',
  'user_message.invalid_operation_reference': 'Action introuvable.',
  'user_message.response_serialization_failed': 'Échec du traitement de la requête.',
  'user_message.plan_changed_rebuild': 'La tâche est obsolète. Veuillez réessayer.',
  'user_message.game_not_in_catalog': 'Le jeu n’est pas pris en charge.',
  'user_message.operation_not_found': 'Action introuvable.',
  'user_message.artifact_not_found': 'Élément introuvable.',
  'user_message.component_not_found': 'Composant introuvable.',
  'user_message.invalid_operation_state': 'Cette action est actuellement indisponible.',
  'user_message.operation_could_not_complete': 'Échec de l’exécution de l’action.',
  'user_message.command_task_failed': 'Échec de l’exécution de la commande.',
  'user_message.storage_failed': 'L’application n’a pas pu lire ou écrire son catalogue.',
  'user_message.provider_failed': 'Impossible de lire une source de données.',
  'user_message.detection_failed': 'L’application n’a pas pu analyser les fichiers du jeu.',
  'user_message.steamgriddb_api_key_missing':
    'Veuillez fournir une clé API SteamGridDB dans les paramètres.',
  'user_message.unsupported_cover_image_type': 'Format d’image non pris en charge.',
  'user_message.cover_download_failed': 'Échec du téléchargement de la jaquette.',
  'user_message.cover_artwork_not_found': 'Aucune jaquette trouvée pour ce jeu.',
  'user_message.cover_file_system_error': 'Échec de l’enregistrement de la jaquette sur le disque.',
  'user_message.stale_replacement_source':
    'Cette mise à jour n’a pas pu être appliquée car le fichier source a été remplacé ou modifié en dehors de RenderPilot. Sélectionnez à nouveau la version — un téléchargement peut être nécessaire.',
  'user_message.nvapi_requires_administrator':
    'Les droits d’administrateur sont requis pour modifier ce paramètre.',

  'suggested_action.refresh_games': 'Actualisez la liste des jeux et réessayez.',
  'suggested_action.reload_game_details': 'Actualisez les détails du jeu et réessayez.',
  'suggested_action.refresh_candidates': 'Actualisez la liste et réessayez.',
  'suggested_action.rebuild_plan_or_reload_operations': 'Actualisez la vue et réessayez.',
  'suggested_action.retry_after_required_data': 'Veuillez patienter et réessayer plus tard.',
  'suggested_action.inspect_logs': 'Si le problème persiste, essayez de redémarrer l’application.',
  'suggested_action.retry_or_restart':
    'Si le problème persiste, essayez de redémarrer l’application.',
  'suggested_action.rebuild_operation_plan': 'Veuillez recommencer l’action.',
  'suggested_action.refresh_or_scan_game_folder':
    'Actualisez la liste ou analysez à nouveau le dossier.',
  'suggested_action.relaunch_as_administrator':
    'Redémarrez l’application en tant qu’administrateur et réessayez.',

  'settings.about.title': 'Mises à jour',
  'settings.about.description': 'Rechercher des mises à jour.',
  'settings.about.version.title': "Version de l'application",
  'settings.about.version.loading': 'Chargement...',
  'settings.about.checkForUpdates': 'Rechercher des mises à jour',
  'settings.about.updateInProgress': 'Mise à jour…',
  'settings.about.updateAvailable': 'Mise à jour disponible',
  'settings.about.upToDate': 'Vous avez la dernière version',
  'settings.about.updateCheckError': 'Erreur lors de la recherche de mises à jour',

  'settings.about.updateDialog.title': 'Mise à jour disponible',
  'settings.about.updateDialog.versionLine': '{currentVersion} → {version}',
  'settings.about.updateDialog.releaseDate': 'Publiée le {date}',
  'settings.about.updateDialog.releaseNotes': 'Notes de version',
  'settings.about.updateDialog.noNotes':
    "Aucune note de version n'a été fournie pour cette mise à jour.",
  'settings.about.updateDialog.notesTruncated': 'Les notes de version ont été raccourcies.',

  'settings.about.updateDialog.installAndRestart': 'Installer et redémarrer',
  'settings.about.updateDialog.later': 'Plus tard',
  'settings.about.updateDialog.close': 'Fermer',
  'settings.about.updateDialog.retryDownload': 'Réessayer le téléchargement',
  'settings.about.updateDialog.retryInstall': "Réessayer l'installation",
  'settings.about.updateDialog.restartNow': 'Redémarrer maintenant',

  'settings.about.updateDialog.downloading': 'Téléchargement de la mise à jour…',
  'settings.about.updateDialog.downloadingBytes': '{received} téléchargés',
  'settings.about.updateDialog.downloadingBytesTotal': '{received} sur {total}',
  'settings.about.updateDialog.verifying': 'Vérification de la mise à jour…',
  'settings.about.updateDialog.verifyingDescription': 'Vérification du paquet téléchargé.',
  'settings.about.updateDialog.installing':
    'Installation de la mise à jour… L’application va se fermer ; l’installateur peut apparaître brièvement.',
  'settings.about.updateDialog.restarting': "Redémarrage de l'application…",

  'settings.about.updateDialog.prepareErrorTitle': 'Échec du téléchargement ou de la vérification',
  'settings.about.updateDialog.prepareErrorDescription':
    "La mise à jour n'a pas pu être téléchargée ou vérifiée. Vérifiez votre connexion et réessayez.",
  'settings.about.updateDialog.installErrorTitle': "Échec de l'installation",
  'settings.about.updateDialog.installErrorDescription':
    "La mise à jour n'a pas pu être installée. Vous pouvez réessayer.",
  'settings.about.updateDialog.restartRequiredTitle': 'Redémarrage requis',
  'settings.about.updateDialog.restartRequiredDescription':
    "La mise à jour a été installée, mais l'application n'a pas pu redémarrer automatiquement. Redémarrez RenderPilot manuellement pour terminer la mise à jour.",

  'settings.about.updateDialog.progressAria': 'Progression du téléchargement',
  // ── Game details: RenoDX ──
  'gameDetails.renodx.title': 'RenoDX HDR',
  'gameDetails.renodx.description':
    'Ajoutez le HDR et le tone-mapping à ce jeu via le module ReShade RenoDX.',
  'gameDetails.renodx.loading': 'Vérification de la disponibilité…',
  'gameDetails.renodx.installError': 'Échec de l’installation de RenoDX',
  'gameDetails.renodx.uninstallError': 'Échec de la suppression de RenoDX',
  'gameDetails.renodx.switchError': 'Échec du changement de canal ReShade',
  'gameDetails.renodx.unsupported': 'Aucun profil RenoDX n’est disponible pour ce jeu.',
  'gameDetails.renodx.incompatible': 'Impossible d’installer RenoDX : {reason}.',
  'gameDetails.renodx.status.label': 'Status',
  'gameDetails.renodx.statusInstalled': 'Installé',
  'gameDetails.renodx.actionInstall': 'Installer',
  'gameDetails.renodx.actionUninstall': 'Supprimer RenoDX',
  'gameDetails.renodx.actionRepair': 'Réparer',
  'gameDetails.renodx.uninstallConfirmTitle': 'Supprimer RenoDX de ce jeu ?',
  'gameDetails.renodx.uninstallConfirmBody':
    'Cela supprime l’add-on RenoDX et restaure uniquement les fichiers ReShade modifiés pendant la configuration de RenoDX.',
  'gameDetails.renodx.uninstallConfirmAction': 'Supprimer',
  'gameDetails.renodx.installing': 'Installation…',
  'gameDetails.renodx.confirmTitle': 'Installer RenoDX malgré le risque anti-triche ?',
  'gameDetails.renodx.cancel': 'Annuler',
  // ── Game details: RenoDX shared Vulkan layer ──
  'gameDetails.renodx.vulkanLayer.removeError':
    'Impossible de supprimer la couche Vulkan ReShade partagée.',
  'gameDetails.renodx.vulkanLayer.title': 'Couche Vulkan partagée',
  'gameDetails.renodx.vulkanLayer.removeConfirmTitle': 'Supprimer la couche Vulkan partagée ?',
  'gameDetails.renodx.vulkanLayer.removeConfirmBody':
    'Supprimer la couche Vulkan ReShade partagée affecte tous les jeux Vulkan RenoDX. Continuer ?',
  'gameDetails.renodx.vulkanLayer.openSettings': 'Ouvrir les paramètres RenoDX',
  'gameDetails.renodx.vulkanLayer.externalReadOnly':
    'Couche Vulkan existante détectée ; lecture seule dans cette version',
  'gameDetails.renodx.vulkanLayer.state.not_installed': 'Non installée',
  'gameDetails.renodx.vulkanLayer.state.installed': 'Installée',
  'gameDetails.renodx.vulkanLayer.state.installed_disabled': 'Disabled in registry',
  'gameDetails.renodx.vulkanLayer.state.external_read_only': 'Lecture seule',
  'gameDetails.renodx.vulkanLayer.state.conflict': 'Conflit',
  'gameDetails.renodx.vulkanLayer.state.needs_repair': 'Réparation requise',
  'gameDetails.renodx.vulkanLayer.state.unsupported': 'Non prise en charge',
  'gameDetails.renodx.vulkanLayer.action.install': 'Installer',
  'gameDetails.renodx.vulkanLayer.action.update': 'Mettre à jour',
  'gameDetails.renodx.vulkanLayer.action.switch_channel': 'Changer de canal',
  'gameDetails.renodx.vulkanLayer.action.repair': 'Réparer la couche',
  'gameDetails.renodx.vulkanLayer.action.remove': 'Supprimer',
  'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected':
    'Une couche Vulkan existante a été détectée.',
  'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest':
    'Plusieurs manifestes de couche ReShade sont enregistrés.',
  'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility':
    'La visibilité du chargeur est ambiguë.',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll':
    'La DLL de la couche est manquante.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll':
    'The layer DLL could not be read (permission denied or locked).',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest': 'The layer manifest is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing':
    'Les fichiers de couche existent, mais l’enregistrement du chargeur Vulkan manque.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled':
    'The loader registry entry is disabled.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture':
    'L’architecture de la couche n’est pas prise en charge.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated':
    'La couche est enregistrée sous HKCU et peut ne pas se charger pour les jeux lancés avec élévation.',
  'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed':
    'Un manifeste de couche n’a pas pu être analysé.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable':
    'La portée de registre requise ne peut pas être écrite.',
  'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied':
    'Le système d’exploitation a refusé une opération requise.',
  'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed':
    'La validation du backend a échoué ; la couche doit être révisée.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch':
    'The layer DLL hash does not match the expected version.',
  'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback':
    'The layer DLL is missing; using advisory database record.',
  // ── Game details: RenoDX incompatibility reasons ──
  'gameDetails.renodx.reason.api_unsupported': 'API graphique non prise en charge',
  'gameDetails.renodx.reason.api_not_allowed': 'API graphique non autorisée pour ce jeu',
  'gameDetails.renodx.reason.arch_unknown': 'architecture de l’exécutable inconnue',
  'gameDetails.otherTab': 'Autres',
  'gameDetails.renodx.unavailable': 'RenoDX est indisponible pour le moment.',
  'renodx.generic.universal': 'RenoDX universel',
  'renodx.generic.unity': 'RenoDX universel (Unity)',
  'gameDetails.renodx.generic.profileTooltip': 'Un profil partagé du moteur est utilisé.',
  'renodx.phase.finalizing': 'Finalisation…',
  'luma.phase.finalizing': 'Finalisation…',
  'gameDetails.renodx.confidenceLabel': 'Compatibilité RenoDX',
  'gameDetails.renodx.confidenceVerified': 'Fonctionne',
  'gameDetails.renodx.confidenceExperimental': 'En cours',
  'gameDetails.renodx.confidenceUntested': 'Non vérifié',
  'gameDetails.renodx.external':
    'Ce module RenoDX est distribué en externe et doit être téléchargé manuellement.',
  'gameDetails.renodx.actionOpenExternal': 'Ouvrir la page de téléchargement',
  'gameDetails.renodx.external.installFromFile': 'Installer depuis un fichier',
  'gameDetails.renodx.external.dropHint':
    'Téléchargez le module, puis déposez-le ici ou sélectionnez le fichier.',
  'gameDetails.renodx.external.invalidFile':
    'Ce fichier n’est pas un module RenoDX (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.title': 'Installation manuelle',
  'gameDetails.renodx.fileInstall.chooseFile': 'Choisir le fichier d’add-on…',
  'gameDetails.renodx.fileInstall.chooseAnother': 'Choisir un autre fichier',
  'gameDetails.renodx.fileInstall.expected': 'Add-on attendu : {name}',
  'gameDetails.renodx.fileInstall.confirm': 'Installer {fileName} ?',
  'gameDetails.renodx.fileInstall.errorExtension':
    'Ce fichier n’est pas un add-on RenoDX (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.errorArch':
    'Cet add-on est {addon} mais le jeu est {game}. Téléchargez l’add-on correspondant.',
  'gameDetails.renodx.fileInstall.warnName':
    'Cela ne ressemble pas à l’add-on attendu ({expected}). N’installez que si vous êtes sûr.',
  'gameDetails.renodx.nativeHdr':
    'Ce jeu prend déjà en charge le HDR natif — RenoDX n’est pas nécessaire.',
  'gameDetails.renodx.blacklisted': 'RenoDX n’est pas recommandé pour ce jeu.',
  'gameDetails.renodx.updatesNotTracked': 'Mises à jour non suivies',
  'gameDetails.renodx.channel.label': 'Canal de l’hôte ReShade',
  'gameDetails.renodx.channel.hostLabel': 'Hôte ReShade',
  'gameDetails.renodx.channel.stable': 'Stable',
  'gameDetails.renodx.channel.nightly': 'Nightly',
  'gameDetails.renodx.host.version': '{version}',
  'gameDetails.renodx.host.versionUnknown': 'version inconnue',
  'gameDetails.renodx.host.addons.none': 'add-ons non pris en charge',
  'gameDetails.renodx.host.addons.unknown': 'prise en charge des add-ons inconnue',
  'gameDetails.renodx.host.action.update_host': 'mise à jour disponible',
  'gameDetails.renodx.host.action.repair_host':
    'Réparer ReShade pour la prise en charge des add-ons RenoDX',
  'gameDetails.renodx.host.customBuild':
    'Version personnalisée (ex. GShade) — vous la mettez à jour vous-même',
  'gameDetails.renodx.host.conflictMultiple':
    'Plusieurs hôtes ReShade trouvés — vérifiez l’emplacement actif',
  'gameDetails.renodx.host.conflictBlocksInstall':
    'Un fichier occupe déjà l’emplacement ReShade utilisé par ce jeu, ou ReShade est dans un autre emplacement — à résoudre avant l’installation.',
  'gameDetails.renodx.actionUpdate': 'Mettre à jour',
  'gameDetails.renodx.updating': 'Mise à jour…',
  'gameDetails.renodx.updateError': 'Échec de la mise à jour de RenoDX',
  'gameDetails.renodx.actionInstallDlssFix': 'Installer',
  'gameDetails.renodx.actionRemoveDlssFix': 'Supprimer',
  'gameDetails.renodx.dlssFixInstallError': "Échec de l'installation de DLSS-Fix",
  'gameDetails.renodx.dlssFixRemoveError': 'Échec de la désinstallation de DLSS-Fix',
  'gameDetails.renodx.fresh.label': 'Mises à jour',
  'gameDetails.renodx.fresh.current': 'À jour',
  'gameDetails.renodx.fresh.available': 'Mise à jour disponible',
  'gameDetails.renodx.fresh.channelMismatch': 'Changement de canal disponible',
  'gameDetails.renodx.fresh.validationRequired': 'Validation requise',
  'gameDetails.renodx.fresh.unknown': 'Vérification impossible',
  'gameDetails.renodx.fresh.checking': 'Vérification…',
  'gameDetails.renodx.addonDated': 'Add-on daté du {date}',
  'gameDetails.renodx.installedOn': 'Installé le {date}',
  'gameDetails.renodx.lastChecked': 'Vérifié {time}',
  'gameDetails.renodx.lastCheckedNever': 'Pas encore vérifié',
  'gameDetails.renodx.actionCheckUpdates': 'Rechercher des mises à jour',
  'gameDetails.renodx.component.reshade': 'Hôte ReShade',
  'gameDetails.renodx.component.addon': 'Add-on RenoDX',
  'gameDetails.renodx.component.addonDesc': "L'add-on HDR pour ce jeu",
  'gameDetails.renodx.component.addonDisabled': 'Installé, mais désactivé dans ReShade.ini',
  'gameDetails.renodx.component.addonFileInstall':
    'Installé depuis un fichier — pas de suivi des mises à jour',
  'gameDetails.renodx.component.dlssFix': 'DLSS-Fix',
  'gameDetails.renodx.component.dlssFixDesc': 'Corrige le scintillement avec DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixOffer':
    'Disponible — évite le scintillement avec DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixHint':
    "Un correctif ReShade général, pas spécifique à RenoDX. Il fait dessiner ReShade sur les images natives du jeu plutôt que sur celles de Frame Generation, et masque l'upscaling DLSS à ReShade lorsque le jeu implémente Streamline correctement.",
  'gameDetails.renodx.attribution': 'RenoDX par clshortfuse.',
  'gameDetails.renodx.attributionLink': 'Voir le projet',
  // ── Game details: shared add-on copy (RenoDX + Luma) ──
  'gameDetails.addon.riskSafe': 'Aucun anti-triche détecté — installation sûre.',
  'gameDetails.addon.riskWarn':
    'Anti-triche détecté — l’installation peut entraîner un bannissement.',
  'addon.risk.sp_safe':
    'Aucune signature anti-triche connue détectée — l’installation de {addonName} est probablement sûre, mais pas garantie.',
  'addon.risk.anticheat_detected':
    'Signatures anti-triche détectées — l’installation de {addonName} peut entraîner un bannissement.',
  'gameDetails.addon.confirmAccept': 'Installer quand même',
  'gameDetails.addon.confirmBody':
    'Ce jeu utilise un anti-triche. L’add-on ReShade pourrait le déclencher et entraîner un bannissement. Continuez à vos risques et périls.',
  'gameDetails.addon.fullAddonWarning':
    'La prise en charge complète des add-ons ReShade peut être risquée pour les jeux multijoueurs ou protégés par anti-triche.',
  'gameDetails.addon.blockedByOtherAddon.tracked':
    '{installedAddon} est installé pour ce jeu — désinstallez-le avant d’installer {blockedAddon}.',
  'gameDetails.addon.blockedByOtherAddon.unmanaged':
    'Des fichiers {installedAddon} ont été trouvés sur le disque pour ce jeu — supprimez-les avant d’installer {blockedAddon}.',
  'addon.availability.loadFailed': 'Impossible de vérifier',
  'addon.availability.retry': 'Réessayer',
  'addon.availability.checking': 'Vérification…',
  // ── Game details: Luma ──
  'gameDetails.luma.title': 'Luma Framework',
  'gameDetails.luma.description':
    'Les fonctionnalités Luma disponibles pour ce jeu sont indiquées ci-dessous.',
  'gameDetails.luma.loading': 'Vérification de la disponibilité…',
  'gameDetails.luma.installError': 'Échec de l’installation de Luma',
  'gameDetails.luma.uninstallError': 'Échec de la désinstallation de Luma',
  'gameDetails.luma.updateError': 'Échec de la mise à jour de Luma',
  'gameDetails.luma.repairError': 'Échec de la réparation de Luma',
  'gameDetails.luma.unsupported': 'Aucun profil Luma n’est disponible pour ce jeu.',
  'gameDetails.luma.incompatible': 'Luma ne peut pas être installé : {reason}.',
  'gameDetails.luma.blacklisted': 'Luma n’est pas recommandé pour ce jeu.',
  'gameDetails.luma.unavailable': 'Luma est actuellement indisponible.',
  'gameDetails.luma.unmanagedPresent':
    'Une installation Luma existante a été trouvée sur le disque sans enregistrement suivi. Supprimez-la manuellement, puis réinstallez.',
  'gameDetails.luma.installTornWarning':
    'Une installation précédente ne s’est pas terminée proprement. Réinstaller la nettoiera et la réparera.',
  'gameDetails.luma.installTornWarningInstalled':
    'La dernière opération ne s’est pas terminée proprement. Utilisez Réparer (ou Mettre à jour si affiché) pour terminer la réconciliation de l’installation.',
  'gameDetails.luma.status.label': 'Statut',
  'gameDetails.luma.statusInstalled': 'Installé',
  'gameDetails.luma.actionInstall': 'Installer',
  'gameDetails.luma.installing': 'Installation…',
  'gameDetails.luma.actionUninstall': 'Supprimer Luma',
  'gameDetails.luma.actionRepair': 'Réparer',
  'gameDetails.luma.actionUpdate': 'Mettre à jour',
  'gameDetails.luma.updating': 'Mise à jour…',
  'gameDetails.luma.actionCheckUpdates': 'Rechercher des mises à jour',
  'gameDetails.luma.uninstallConfirmTitle': 'Supprimer Luma de ce jeu ?',
  'gameDetails.luma.uninstallConfirmBody':
    'Cela supprime Luma. Si Luma gère la DLL DLSS, son Library Swap est annulé et l’état exact antérieur à Luma est restauré. Les DLL réutilisées et les swaps indépendants restent inchangés.',
  'gameDetails.luma.uninstallConfirmAction': 'Supprimer',
  'gameDetails.luma.confirmTitle': 'Installer Luma malgré le risque lié à l’anti-triche ?',
  'gameDetails.luma.vcredistWarning':
    'Un Visual C++ Redistributable récent semble manquer sur ce système. Si Luma ne se charge pas, installez le redistribuable.',
  'gameDetails.luma.vcredistLink': 'Télécharger le redistribuable',
  'gameDetails.luma.dgvoodoo.managed':
    'RenderPilot installera et configurera dgVoodoo2 {version} pour ce profil Luma.',
  // ── Game details: Luma confidence ──
  'gameDetails.luma.confidenceLabel': 'Compatibilité Luma',
  'gameDetails.luma.confidenceVerified': 'Fonctionne',
  'gameDetails.luma.confidenceExperimental': 'En cours',
  'gameDetails.luma.confidenceUntested': 'Non vérifié',
  'gameDetails.luma.generic.engineUnreal': 'Unreal Engine',
  'gameDetails.luma.generic.engineUnity': 'Unity',
  'gameDetails.luma.generic.profileTooltip': 'Un profil partagé du moteur est utilisé.',
  'gameDetails.luma.features.title': 'Fonctionnalités',
  'gameDetails.luma.features.dlssFsr': 'DLSS / FSR',
  'gameDetails.luma.features.hdr': 'HDR',
  'gameDetails.luma.features.supported': 'Pris en charge',
  'gameDetails.luma.features.unsupported': 'Non pris en charge',
  'gameDetails.luma.features.experimental': 'Expérimental',
  'gameDetails.luma.features.unknown': 'Inconnu',
  // ── Game details: Luma incompatibility reasons ──
  'gameDetails.luma.reason.api_unsupported': 'API graphique non prise en charge',
  'gameDetails.luma.reason.api_not_allowed': 'API graphique non autorisée pour ce jeu',
  'gameDetails.luma.reason.arch_unknown': 'architecture de l’exécutable inconnue',
  'gameDetails.luma.reason.arch_mismatch':
    'l’architecture de l’exécutable ne correspond pas à cet add-on',
  // ── Game details: Luma ReShade host ──
  'gameDetails.luma.channel.stable': 'Stable',
  'gameDetails.luma.channel.nightly': 'Nightly',
  'gameDetails.luma.host.version': '{version}',
  'gameDetails.luma.host.versionUnknown': 'Version inconnue',
  'gameDetails.luma.host.addons.none': 'add-ons non pris en charge',
  'gameDetails.luma.host.addons.unknown': 'prise en charge des add-ons inconnue',
  'gameDetails.luma.host.action.update_host': 'mise à jour disponible',
  'gameDetails.luma.host.action.repair_host':
    'Réparer ReShade pour la prise en charge de l’add-on Luma',
  'gameDetails.luma.host.customBuild':
    'Version personnalisée (par ex. GShade) — vous gérez vous-même ses mises à jour',
  'gameDetails.luma.host.conflictMultiple':
    'Plusieurs hôtes ReShade trouvés — l’emplacement actif doit être vérifié',
  'gameDetails.luma.host.conflictBlocksInstall':
    'Un fichier existant occupe l’emplacement ReShade utilisé par ce jeu, ou ReShade se trouve dans un autre emplacement — résolvez cela avant d’installer.',
  // ── Game details: Luma freshness / timestamps ──
  'gameDetails.luma.fresh.label': 'Version',
  'gameDetails.luma.fresh.current': 'À jour',
  'gameDetails.luma.fresh.available': 'Mise à jour disponible',
  'gameDetails.luma.fresh.channelMismatch': 'Changement de canal disponible',
  'gameDetails.luma.fresh.validationRequired': 'Validation requise',
  'gameDetails.luma.fresh.unknown': 'Impossible de vérifier',
  'gameDetails.luma.fresh.checking': 'Vérification…',
  'gameDetails.luma.updatesNotTracked': 'Mises à jour non suivies',
  'gameDetails.luma.addonDated': 'Add-on daté du {date}',
  'gameDetails.luma.installedOn': 'Installé le {date}',
  'gameDetails.luma.lastChecked': 'Vérifié {time}',
  'gameDetails.luma.lastCheckedNever': 'Pas encore vérifié',
  // ── Game details: Luma components ──
  'gameDetails.luma.component.reshade': 'Hôte ReShade',
  'gameDetails.luma.component.addon': 'Add-on Luma',
  'gameDetails.luma.component.addonDesc': 'Fonctionnalités Luma pour ce jeu',
  'gameDetails.luma.component.dgvoodoo': 'Wrapper dgVoodoo2',
  'gameDetails.luma.component.dgvoodooDesc': 'Pont D3D9 géré, version {version}',
  // ── Game details: Luma launch arguments ──
  'gameDetails.luma.launchArgs.instructions.steam':
    'Si vous lancez le jeu via Steam, ajoutez-les ici : clic droit sur le jeu → Propriétés → Général → Options de lancement.',
  'gameDetails.luma.launchArgs.instructions.gog':
    'Si vous lancez le jeu via GOG Galaxy, ajoutez-les ici : paramètres du jeu → Gérer l’installation → Configurer.',
  'gameDetails.luma.launchArgs.instructions.epic':
    'Si vous lancez le jeu via l’Epic Games Launcher, ajoutez-les ici : clic droit sur le jeu → Gérer → Arguments de ligne de commande supplémentaires.',
  'gameDetails.luma.launchArgs.instructions.ea':
    'Si vous lancez le jeu via l’application EA, ajoutez-les ici : sélectionnez le jeu → Gérer → Voir les propriétés → Options de lancement avancées.',
  'gameDetails.luma.launchArgs.instructions.ubisoft':
    'Si vous lancez le jeu via Ubisoft Connect, ajoutez-les ici : sélectionnez le jeu → Propriétés → Ajouter des arguments de lancement.',
  'gameDetails.luma.launchArgs.instructions.other':
    'Utilisez la méthode qui lance réellement le jeu. Ajoutez les arguments dans son launcher, la cible du raccourci, un fichier batch ou un autre chargeur.',
  'gameDetails.luma.launchArgs.title': 'Arguments de lancement requis',
  'gameDetails.luma.launchArgs.dx11Title': 'Ce profil Luma nécessite DirectX 11',
  'gameDetails.luma.launchArgs.copyStep': 'Copiez les arguments de lancement requis :',
  'gameDetails.luma.launchArgs.copy': 'Copier les arguments',
  'gameDetails.luma.launchArgs.copied': 'Copié',
  'gameDetails.luma.launchArgs.copyFailed': 'Impossible de copier les arguments de lancement',
  // ── Game details: Luma attribution ──
  'gameDetails.luma.attribution': 'Luma Framework par Filoppi.',
  'gameDetails.luma.attributionLink': 'Voir le projet',
  'gameDetails.luma.guidance.gameSetting': 'Réglage en jeu',
  'gameDetails.luma.guidance.engineIni': 'Modification manuelle de l’INI',
  'gameDetails.luma.guidance.launchArgument': 'Argument de lancement',
  'gameDetails.luma.guidance.warning': 'Important',
  'gameDetails.luma.guidance.compatibility': 'Note de compatibilité',
  'gameDetails.luma.guidance.externalTool': 'Outil tiers',
  'gameDetails.luma.guidance.copy': 'Copier',
  'gameDetails.luma.guidance.copied': 'Copié',
  'gameDetails.luma.guidance.copyFailed': 'Impossible de copier',
});
