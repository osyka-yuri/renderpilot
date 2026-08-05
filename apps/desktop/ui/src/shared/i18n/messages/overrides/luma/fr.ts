import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
  sherlockDx11Performance:
    'L’argument de lancement -dx11 peut réduire les performances du processeur en mode DX11. Avec DLAA et AutoExposure:On, des bords crénelés peuvent apparaître sur l’herbe.',
  guiltyGearStriveAa:
    'L’anticrénelage ne fonctionne pas sur l’écran de sélection du personnage. Dans le jeu, choisissez AA « Temporal Anti Aliasing », puis ajoutez à Engine.ini : [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
  manualLaunchArgument: 'Ajoutez cet argument de lancement manuellement.',
  manualEngineIni: 'Appliquez manuellement les réglages suivants dans Engine.ini.',
  publicMatchmaking:
    'Évitez le matchmaking public officiel tant que Luma est installé. Cela peut entraîner un bannissement.',
  edithFinchExit:
    'DLAA fonctionne sans modification supplémentaire, mais le jeu peut ne pas se fermer complètement après avoir quitté. OptiScaler peut résoudre ce problème.',
  dlssNoHdr: 'DLSS uniquement (pas de HDR pour le moment).',
  fallout4DlssGtaoOnly: 'Ce profil ne prend actuellement en charge que DLSS et GTAO.',
  kh3Txaa: 'Choisissez d’abord « TXAA » dans le jeu.',
  aceFxaaHigh: 'Choisissez d’abord AA « FXAA High » dans le jeu.',
  tetrisFxaa6: 'Choisissez d’abord AA « FXAA:6 » et une échelle de rendu de 100 % dans le jeu.',
  projectWingmanFxaa: 'Choisissez d’abord AA « FXAA » dans le jeu.',
  dnfCharacterSelection:
    'L’anticrénelage ne fonctionne pas sur l’écran de sélection du personnage.',
  tekkenNoD3D9Ex: 'Comme prérequis, ajoutez l’argument de lancement -nod3d9ex.',
  scornOptiscaler:
    'Le jeu prend nativement en charge FSR 2.1 ; vous pouvez donc ajouter DLSS ou un autre upscaler via OptiScaler.',
  hatsuneExclusiveFullscreen:
    'En cas de problème, n’utilisez pas le plein écran exclusif. Appuyez sur Alt+Entrée pour le quitter.',
  deadlineUltra: 'Dans les réglages du jeu, choisissez « Ultra ».',
  filamentAaHigh: 'Dans les réglages du jeu, choisissez AA « High » ou « Very High ».',
  aaHigh: 'Dans les réglages du jeu, choisissez AA « High ».',
  mutantMotionBlur:
    'Dans les réglages du jeu, choisissez AA « High ». Pour des mouvements plus nets, r.motionblur.amount=0 est recommandé dans Engine.ini.',
  supralandTaa: 'Dans les réglages du jeu, choisissez AA « Temporal Anti Aliasing ».',
  scarletNexusTxaa: 'Dans les réglages du jeu, choisissez AA « TXAA ».',
  closeToSunAa4x: 'Dans les réglages du jeu, choisissez AA 4X.',
  darksidersAaEpic: 'Dans les réglages du jeu, choisissez AA Epic.',
  codeVeinAaHighest: 'Dans les réglages du jeu, choisissez AA Highest.',
  orcsAaHigh: 'Dans les réglages du jeu, choisissez la qualité AA « High ».',
  clashAaVeryHigh: 'Dans les réglages du jeu, choisissez la qualité AA « Very High ».',
  vampyrTxaa6x: 'Dans les réglages du jeu, choisissez AA TXAA 6X.',
  crashAaMedium:
    'Dans les réglages du jeu, choisissez au moins la qualité d’anticrénelage Medium (2x).',
  callSeaEpic: 'Dans les réglages du jeu, choisissez la qualité globale « Epic ».',
  spiritNorthUltra: 'Dans les réglages du jeu, choisissez la qualité graphique « Ultra ».',
  goatHighAa: 'Dans les réglages du jeu, choisissez High AA.',
  crabHighAntialiasing: 'Dans les réglages du jeu, choisissez High Anti-aliasing Type.',
  spyroHighTaa: 'Dans les réglages du jeu, choisissez High TAA.',
  dieYoungTaa: 'Dans les réglages du jeu, choisissez TAA « High » ou « Epic ».',
  kakarotBdzKfix:
    'Dans les réglages du jeu, choisissez TAA et utilisez BDZKFix pour la version Legacy ou son fork mis à jour pour la version HD.',
  preyData: 'Conservez les fichiers de données Luma supplémentaires de Prey avec l’add-on.',
  daymareOptiscalerUuu:
    'Luma fonctionne seul, mais plante lorsqu’il est combiné avec OptiScaler ou UUU.',
  smtLyallFix: "Lyall's Fix est nécessaire pour forcer TAA.",
  deusExBorisEnb:
    'Incompatible avec Boris ENB (DX9). Fonctionne avec DE et l’édition originale. Le mod Gold Filter Restoration est redondant.',
  xboxStore: 'Incompatible avec la version Xbox Store.',
  massEffectNativeAa:
    'Seuls les modes DLAA / FSR 3 Native AA sont disponibles ; il ne s’agit pas de super-résolution DLSS ou FSR.',
  metaphorNativeAa:
    'Seuls les modes DLAA / FSR Native AA sont disponibles ; il ne s’agit pas de super-résolution DLSS ou FSR.',
  itTakesTwoTitle: 'Fonctionne uniquement pendant la séquence de l’écran-titre.',
  talesAriseSdk: 'Nécessite Arise-SDK avec UseUE4TAA = true.',
  metroWindowed:
    'Nécessite le mode fenêtré ou fenêtré sans bordure, via des mods ou en désactivant le plein écran dans le fichier de configuration du jeu.',
  edithFinch4k:
    'Le jeu ne fonctionne pas correctement en 4K. Réglez Effects sur Low avant de modifier Engine.ini manuellement.',
  sinkingCityOriginal:
    'La version originale fonctionne. L’état de la version remasterisée est inconnu.',
  heavyRainSteamUltrawide: 'L’ultralarge peut ne fonctionner qu’en lançant le jeu via Steam.',
  metroBorderless: 'Utilisez le mode fenêtré sans bordure.',
  dlssOnlyNoHdr:
    'Ce profil ajoute uniquement la prise en charge de DLSS ; HDR n’est pas pris en charge actuellement.',
  biomutantAaHighOrMax: 'Dans les réglages du jeu, utilisez AA « High » ou « Max ».',
  blairWitchTxaaFull:
    'Dans les réglages du jeu, utilisez TXAA et l’échelle de résolution « Full ».',
  flickeringIssues: 'Des problèmes de scintillement semblent se produire.',
  brambleEpicVram:
    'La qualité Epic peut remplir progressivement la VRAM et provoquer des saccades. Évitez de basculer plusieurs fois entre High et Epic lorsque Luma est actif.',
  daemonDlaaReset:
    'Charger un niveau ou modifier les réglages graphiques force r.TemporalAASamples=1 et désactive DLAA.',
  easyAntiCheatBlocked: 'Bloqué par Easy Anti-Cheat.',
  echoDlaaAutoExposure:
    'DLAA cesse de fonctionner après le premier niveau. Avec AutoExposure: On, les sources lumineuses scintillent ; avec AutoExposure: Off, l’anticrénelage se dégrade fortement.',
  dx11BootFailure: 'Ne démarre pas avec DX11.',
  rainCodeAaHighMaxResolution:
    'Dans les réglages du jeu, utilisez la qualité AA « High » et placez le curseur de résolution au maximum.',
  roboquestTaaQuality3: 'Dans les réglages du jeu, utilisez TAA et la qualité « 3 ».',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'fr', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
