import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
  sherlockDx11Performance:
    '启动参数 -dx11 可能降低 DX11 模式下的 CPU 性能。使用 DLAA 和 AutoExposure:On 时，草地边缘可能出现锯齿。',
  guiltyGearStriveAa:
    '角色选择界面中抗锯齿无效。在游戏内选择 AA“Temporal Anti Aliasing”，然后在 Engine.ini 中添加：[SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4。',
  manualLaunchArgument: '请手动添加此启动参数。',
  manualEngineIni: '请在 Engine.ini 中手动应用以下设置。',
  publicMatchmaking: '安装 Luma 后请避免使用官方公开匹配。这可能导致封禁。',
  edithFinchExit:
    'DLAA 无需额外修改即可工作，但退出后游戏可能无法完全关闭。OptiScaler 或可解决此问题。',
  dlssNoHdr: '仅支持 DLSS（暂不支持 HDR）。',
  fallout4DlssGtaoOnly: '此配置目前仅支持 DLSS 和 GTAO。',
  kh3Txaa: '请先在游戏内选择“TXAA”。',
  aceFxaaHigh: '请先在游戏内选择 AA“FXAA High”。',
  tetrisFxaa6: '请先在游戏内选择 AA“FXAA:6”并将渲染比例设为 100%。',
  projectWingmanFxaa: '请先在游戏内选择 AA“FXAA”。',
  dnfCharacterSelection: '角色选择界面中抗锯齿无效。',
  tekkenNoD3D9Ex: '作为前置条件，请添加启动参数 -nod3d9ex。',
  scornOptiscaler: '游戏原生支持 FSR 2.1，因此可通过 OptiScaler 添加 DLSS 或其他超分辨率器。',
  hatsuneExclusiveFullscreen: '如出现问题，请不要使用独占全屏。按 Alt+Enter 可退出该模式。',
  deadlineUltra: '请在游戏设置中选择“Ultra”。',
  filamentAaHigh: '请在游戏设置中选择 AA“High”或“Very High”。',
  aaHigh: '请在游戏设置中选择 AA“High”。',
  aaUltra: '请在游戏设置中选择 AA“Ultra”。',
  mutantMotionBlur:
    '请在游戏设置中选择 AA“High”。为获得更清晰的运动画面，建议在 Engine.ini 中设置 r.motionblur.amount=0。',
  supralandTaa: '请在游戏设置中选择 AA“Temporal Anti Aliasing”。',
  scarletNexusTxaa: '请在游戏设置中选择 AA“TXAA”。',
  closeToSunAa4x: '请在游戏设置中选择 AA 4X。',
  darksidersAaEpic: '请在游戏设置中选择 AA Epic。',
  codeVeinAaHighest: '请在游戏设置中选择 AA Highest。',
  orcsAaHigh: '请在游戏设置中选择 AA 质量“High”。',
  clashAaVeryHigh: '请在游戏设置中选择 AA 质量“Very High”。',
  vampyrTxaa6x: '请在游戏设置中选择 AA TXAA 6X。',
  crashAaMedium: '请在游戏设置中至少选择 Medium (2x) 抗锯齿质量。',
  callSeaEpic: '请在游戏设置中选择全局质量“Epic”。',
  spiritNorthUltra: '请在游戏设置中选择图形质量“Ultra”。',
  goatHighAa: '请在游戏设置中选择 High AA。',
  crabHighAntialiasing: '请在游戏设置中选择 High Anti-aliasing Type。',
  spyroHighTaa: '请在游戏设置中选择 High TAA。',
  dieYoungTaa: '请在游戏设置中选择 TAA“High”或“Epic”。',
  kakarotBdzKfix: '请在游戏设置中选择 TAA；Legacy 版本使用 BDZKFix，HD 版本使用其更新分支。',
  preyData: '请将 Prey 的额外 Luma 数据文件与该插件放在一起。',
  daymareOptiscalerUuu: 'Luma 单独使用时正常，但与 OptiScaler 或 UUU 结合时会崩溃。',
  smtLyallFix: "强制启用 TAA 需要 Lyall's Fix。",
  deusExBorisEnb:
    '与 Boris ENB（DX9）不兼容。适用于 DE 和原版；Gold Filter Restoration mod 属于重复功能。',
  xboxStore: '与 Xbox Store 版本不兼容。',
  massEffectNativeAa: '仅提供 DLAA / FSR 3 Native AA 模式；这不是 DLSS 或 FSR 超分辨率。',
  metaphorNativeAa: '仅提供 DLAA / FSR Native AA 模式；这不是 DLSS 或 FSR 超分辨率。',
  itTakesTwoTitle: '仅在标题画面序列中有效。',
  talesAriseSdk: '需要 Arise-SDK，并设置 UseUE4TAA = true。',
  metroWindowed: '需要窗口或无边框窗口模式：可通过模组实现，或在游戏配置文件中关闭全屏。',
  edithFinch4k: '游戏在 4K 下无法正常工作。手动修改 Engine.ini 前，请将 Effects 设为 Low。',
  sinkingCityOriginal: '原版可用，重制版状态未知。',
  heavyRainSteamUltrawide: '超宽屏可能仅在通过 Steam 启动时可用。',
  metroBorderless: '请使用无边框窗口模式。',
  dlssOnlyNoHdr: '此配置仅添加 DLSS 支持；目前不支持 HDR。',
  biomutantAaHighOrMax: '请在游戏设置中将 AA 设为“High”或“Max”。',
  blairWitchTxaaFull: '请在游戏设置中使用 TXAA，并将分辨率缩放设为“Full”。',
  flickeringIssues: '似乎存在闪烁问题。',
  brambleEpicVram:
    'Epic 画质可能会持续占满 VRAM 并导致卡顿。Luma 启用时，请避免反复在 High 和 Epic 之间切换。',
  daemonDlaaReset: '加载关卡或更改图形设置会强制设定 r.TemporalAASamples=1 并禁用 DLAA。',
  easyAntiCheatBlocked: '被 Easy Anti-Cheat 阻止。',
  echoDlaaAutoExposure:
    '通过第一关后，DLAA 会停止工作。AutoExposure: On 时光源会频闪；AutoExposure: Off 时抗锯齿质量会显著下降。',
  dx11BootFailure: '无法在 DX11 下启动。',
  rainCodeAaHighMaxResolution: '请在游戏设置中将 AA 质量设为“High”，并将分辨率滑块调至最大。',
  roboquestTaaQuality3: '请在游戏设置中使用 TAA，并将 Quality 设为“3”。',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'zh-Hans', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
