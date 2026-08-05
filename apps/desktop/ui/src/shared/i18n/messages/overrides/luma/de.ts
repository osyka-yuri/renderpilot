import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
  sherlockDx11Performance:
    'Der Startparameter -dx11 kann die CPU-Leistung im DX11-Modus verringern. Bei DLAA mit AutoExposure:On können an Gras gezackte Kanten auftreten.',
  guiltyGearStriveAa:
    'Antialiasing funktioniert im Charakterauswahlbildschirm nicht. Wähle im Spiel AA „Temporal Anti Aliasing“ und ergänze Engine.ini um: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
  manualLaunchArgument: 'Füge dieses Startargument manuell hinzu.',
  manualEngineIni: 'Übernimm die folgenden Einstellungen manuell in Engine.ini.',
  publicMatchmaking:
    'Vermeide offizielles öffentliches Matchmaking, solange Luma installiert ist. Dies kann zu einer Sperre führen.',
  edithFinchExit:
    'DLAA funktioniert ohne weitere Änderungen, doch das Spiel schließt sich nach dem Beenden möglicherweise nicht vollständig. OptiScaler kann das Problem beheben.',
  dlssNoHdr: 'Nur DLSS (derzeit kein HDR).',
  fallout4DlssGtaoOnly: 'Dieses Profil unterstützt derzeit nur DLSS und GTAO.',
  kh3Txaa: 'Wähle im Spiel zuvor „TXAA“ aus.',
  aceFxaaHigh: 'Wähle im Spiel zuvor AA „FXAA High“ aus.',
  tetrisFxaa6: 'Wähle im Spiel zuvor AA „FXAA:6“ und 100 % Render-Skalierung aus.',
  projectWingmanFxaa: 'Wähle im Spiel zuvor AA „FXAA“ aus.',
  dnfCharacterSelection: 'Antialiasing funktioniert im Charakterauswahlbildschirm nicht.',
  tekkenNoD3D9Ex: 'Füge als Voraussetzung das Startargument -nod3d9ex hinzu.',
  scornOptiscaler:
    'Das Spiel unterstützt FSR 2.1 nativ; DLSS oder einen anderen Upscaler kannst du daher über OptiScaler hinzufügen.',
  hatsuneExclusiveFullscreen:
    'Nutze bei Problemen keinen exklusiven Vollbildmodus. Drücke Alt+Enter, um ihn zu verlassen.',
  deadlineUltra: 'Wähle in den Spieleinstellungen „Ultra“ aus.',
  filamentAaHigh: 'Wähle in den Spieleinstellungen AA „High“ oder „Very High“ aus.',
  aaHigh: 'Wähle in den Spieleinstellungen AA „High“ aus.',
  mutantMotionBlur:
    'Wähle in den Spieleinstellungen AA „High“ aus. Für klarere Bewegung wird r.motionblur.amount=0 in Engine.ini empfohlen.',
  supralandTaa: 'Wähle in den Spieleinstellungen AA „Temporal Anti Aliasing“ aus.',
  scarletNexusTxaa: 'Wähle in den Spieleinstellungen AA „TXAA“ aus.',
  closeToSunAa4x: 'Wähle in den Spieleinstellungen AA 4X aus.',
  darksidersAaEpic: 'Wähle in den Spieleinstellungen AA Epic aus.',
  codeVeinAaHighest: 'Wähle in den Spieleinstellungen AA Highest aus.',
  orcsAaHigh: 'Wähle in den Spieleinstellungen die AA-Qualität „High“ aus.',
  clashAaVeryHigh: 'Wähle in den Spieleinstellungen die AA-Qualität „Very High“ aus.',
  vampyrTxaa6x: 'Wähle in den Spieleinstellungen AA TXAA 6X aus.',
  crashAaMedium:
    'Wähle in den Spieleinstellungen mindestens die Antialiasing-Qualität Medium (2x) aus.',
  callSeaEpic: 'Wähle in den Spieleinstellungen die Gesamtqualität „Epic“ aus.',
  spiritNorthUltra: 'Wähle in den Spieleinstellungen die Grafikqualität „Ultra“ aus.',
  goatHighAa: 'Wähle in den Spieleinstellungen High AA aus.',
  crabHighAntialiasing: 'Wähle in den Spieleinstellungen High Anti-aliasing Type aus.',
  spyroHighTaa: 'Wähle in den Spieleinstellungen High TAA aus.',
  dieYoungTaa: 'Wähle in den Spieleinstellungen TAA „High“ oder „Epic“ aus.',
  kakarotBdzKfix:
    'Wähle in den Spieleinstellungen TAA aus und nutze BDZKFix für die Legacy-Version beziehungsweise dessen aktualisierten Fork für die HD-Version.',
  preyData: 'Bewahre die zusätzlichen Luma-Datendateien für Prey zusammen mit dem Add-on auf.',
  daymareOptiscalerUuu:
    'Luma funktioniert allein, stürzt jedoch in Kombination mit OptiScaler oder UUU ab.',
  smtLyallFix: "Zum Erzwingen von TAA wird Lyall's Fix benötigt.",
  deusExBorisEnb:
    'Nicht kompatibel mit Boris ENB (DX9). Funktioniert mit DE und der Originalausgabe. Der Mod Gold Filter Restoration ist hier überflüssig.',
  xboxStore: 'Nicht kompatibel mit der Xbox-Store-Version.',
  massEffectNativeAa:
    'Nur DLAA / FSR 3 Native AA sind verfügbar; dies ist kein DLSS- oder FSR-Super-Resolution-Modus.',
  metaphorNativeAa:
    'Nur DLAA / FSR Native AA sind verfügbar; dies ist kein DLSS- oder FSR-Super-Resolution-Modus.',
  itTakesTwoTitle: 'Funktioniert nur während der Titelsequenz.',
  talesAriseSdk: 'Erfordert Arise-SDK mit UseUE4TAA = true.',
  metroWindowed:
    'Erfordert Fenster- oder rahmenlosen Fenstermodus, entweder über Mods oder durch Deaktivieren von Vollbild in der Spielkonfiguration.',
  edithFinch4k:
    'Das Spiel funktioniert in 4K nicht korrekt. Stelle Effects auf Low, bevor du Engine.ini manuell änderst.',
  sinkingCityOriginal: 'Die Originalversion funktioniert. Der Remaster-Status ist unbekannt.',
  heavyRainSteamUltrawide: 'Ultrawide funktioniert möglicherweise nur beim Start über Steam.',
  metroBorderless: 'Verwende den rahmenlosen Fenstermodus.',
  dlssOnlyNoHdr:
    'Dieses Profil fügt nur DLSS-Unterstützung hinzu; HDR wird derzeit nicht unterstützt.',
  biomutantAaHighOrMax: 'Wähle in den Spieleinstellungen AA „High“ oder „Max“.',
  blairWitchTxaaFull: 'Wähle in den Spieleinstellungen TXAA und die Auflösungsskalierung „Full“.',
  flickeringIssues: 'Es scheint Probleme mit Flackern zu geben.',
  brambleEpicVram:
    'Die Qualitätsstufe Epic kann den VRAM stetig füllen und Ruckeln verursachen. Wechsle bei aktivem Luma nicht wiederholt zwischen High und Epic.',
  daemonDlaaReset:
    'Das Laden eines Levels oder das Ändern der Grafikeinstellungen erzwingt r.TemporalAASamples=1 und deaktiviert DLAA.',
  easyAntiCheatBlocked: 'Durch Easy Anti-Cheat blockiert.',
  echoDlaaAutoExposure:
    'DLAA funktioniert nach dem ersten Level nicht mehr. Bei AutoExposure: On flackern Lichtquellen; bei AutoExposure: Off wird die Kantenglättung deutlich schlechter.',
  dx11BootFailure: 'Startet nicht mit DX11.',
  rainCodeAaHighMaxResolution:
    'Wähle in den Spieleinstellungen die AA-Qualität „High“ und stelle den Auflösungsregler auf das Maximum.',
  roboquestTaaQuality3: 'Wähle in den Spieleinstellungen TAA und Qualität „3“.',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'de', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
