import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
  sherlockDx11Performance:
    '啟動引數 -dx11 可能降低 DX11 模式下的 CPU 效能。使用 DLAA 和 AutoExposure:On 時，草地邊緣可能出現鋸齒。',
  guiltyGearStriveAa:
    '角色選擇介面中抗鋸齒無效。在遊戲內選擇 AA「Temporal Anti Aliasing」，然後在 Engine.ini 中新增：[SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4。',
  manualLaunchArgument: '請手動新增此啟動引數。',
  manualEngineIni: '請在 Engine.ini 中手動套用以下設定。',
  publicMatchmaking: '安裝 Luma 後，請避免使用官方的公開配對功能，否則帳號可能遭到停權。',
  edithFinchExit:
    'DLAA 無需額外修改即可運作，但結束後遊戲可能無法完全關閉。OptiScaler 或許能解決此問題。',
  dlssNoHdr: '僅支援 DLSS（暫不支援 HDR）。',
  fallout4DlssGtaoOnly: '此設定檔目前僅支援 DLSS 和 GTAO。',
  kh3Txaa: '請先在遊戲內選擇「TXAA」。',
  aceFxaaHigh: '請先在遊戲內選擇 AA「FXAA High」。',
  tetrisFxaa6: '請先在遊戲內選擇 AA「FXAA:6」，並將渲染比例設為 100%。',
  projectWingmanFxaa: '請先在遊戲內選擇 AA「FXAA」。',
  dnfCharacterSelection: '角色選擇介面中抗鋸齒無效。',
  tekkenNoD3D9Ex: '作為前置條件，請新增啟動引數 -nod3d9ex。',
  scornOptiscaler: '遊戲原生支援 FSR 2.1，因此可透過 OptiScaler 加入 DLSS 或其他超解析度升頻工具。',
  hatsuneExclusiveFullscreen:
    '如果發生問題，請避免使用獨佔全螢幕模式。按 Alt+Enter 即可退出此模式。',
  deadlineUltra: '請在遊戲設定中選擇「Ultra」。',
  filamentAaHigh: '請在遊戲設定中選擇 AA「High」或「Very High」。',
  aaHigh: '請在遊戲設定中選擇 AA「High」。',
  mutantMotionBlur:
    '請在遊戲設定中選擇 AA「High」。為獲得更清晰的運動畫面，建議在 Engine.ini 中設定 r.motionblur.amount=0。',
  supralandTaa: '請在遊戲設定中選擇 AA「Temporal Anti Aliasing」。',
  scarletNexusTxaa: '請在遊戲設定中選擇 AA「TXAA」。',
  closeToSunAa4x: '請在遊戲設定中選擇 AA 4X。',
  darksidersAaEpic: '請在遊戲設定中選擇 AA Epic。',
  codeVeinAaHighest: '請在遊戲設定中選擇 AA Highest。',
  orcsAaHigh: '請在遊戲設定中選擇 AA 品質「High」。',
  clashAaVeryHigh: '請在遊戲設定中選擇 AA 品質「Very High」。',
  vampyrTxaa6x: '請在遊戲設定中選擇 AA TXAA 6X。',
  crashAaMedium: '請在遊戲設定中至少選擇 Medium (2x) 抗鋸齒品質。',
  callSeaEpic: '請在遊戲設定中選擇全域品質「Epic」。',
  spiritNorthUltra: '請在遊戲設定中選擇圖形品質「Ultra」。',
  goatHighAa: '請在遊戲設定中選擇 High AA。',
  crabHighAntialiasing: '請在遊戲設定中選擇 High Anti-aliasing Type。',
  spyroHighTaa: '請在遊戲設定中選擇 High TAA。',
  dieYoungTaa: '請在遊戲設定中選擇 TAA「High」或「Epic」。',
  kakarotBdzKfix: '請在遊戲設定中選擇 TAA；Legacy 版本使用 BDZKFix，HD 版本使用其更新分支。',
  preyData: '請將 Prey 的額外 Luma 資料檔案放在該附加元件旁。',
  daymareOptiscalerUuu: 'Luma 可獨立正常運作，但與 OptiScaler 或 UUU 同時使用時會造成遊戲當機。',
  smtLyallFix: "強制啟用 TAA 需要 Lyall's Fix。",
  deusExBorisEnb:
    '與 Boris ENB（DX9）不相容。適用於 DE 與原版；Gold Filter Restoration 模組的功能與其重複。',
  xboxStore: '與 Xbox Store 版本不相容。',
  massEffectNativeAa: '僅提供 DLAA / FSR 3 Native AA 模式；這並非 DLSS 或 FSR 超解析度功能。',
  metaphorNativeAa: '僅提供 DLAA / FSR Native AA 模式；這並非 DLSS 或 FSR 超解析度功能。',
  itTakesTwoTitle: '僅在標題畫面序列中有效。',
  talesAriseSdk: '需要 Arise-SDK，並設定 UseUE4TAA = true。',
  metroWindowed: '需要使用視窗或無邊框視窗模式：可透過模組啟用，或在遊戲設定檔中關閉全螢幕模式。',
  edithFinch4k: '遊戲在 4K 解析度下無法正常運作。手動修改 Engine.ini 前，請將 Effects 設為 Low。',
  sinkingCityOriginal: '原版可用，重製版狀態未知。',
  heavyRainSteamUltrawide: '超寬螢幕模式可能僅在透過 Steam 啟動遊戲時可用。',
  metroBorderless: '請使用無邊框視窗模式。',
  dlssOnlyNoHdr: '此設定檔僅新增 DLSS 支援；目前不支援 HDR。',
  biomutantAaHighOrMax: '請在遊戲設定中將 AA 設為「High」或「Max」。',
  blairWitchTxaaFull: '請在遊戲設定中使用 TXAA，並將解析度縮放設為「Full」。',
  flickeringIssues: '似乎有閃爍問題。',
  brambleEpicVram:
    'Epic 畫質可能會持續占滿 VRAM 並造成卡頓。Luma 啟用時，請避免反覆在 High 與 Epic 之間切換。',
  daemonDlaaReset: '載入關卡或變更圖形設定會強制設定 r.TemporalAASamples=1 並停用 DLAA。',
  easyAntiCheatBlocked: '遭 Easy Anti-Cheat 阻擋。',
  echoDlaaAutoExposure:
    '通過第一關後，DLAA 會停止運作。AutoExposure: On 時光源會頻閃；AutoExposure: Off 時反鋸齒品質會明顯下降。',
  dx11BootFailure: '無法在 DX11 下啟動。',
  rainCodeAaHighMaxResolution: '請在遊戲設定中將 AA 品質設為「High」，並將解析度滑桿調至最大。',
  roboquestTaaQuality3: '請在遊戲設定中使用 TAA，並將 Quality 設為「3」。',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'zh-Hant', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
