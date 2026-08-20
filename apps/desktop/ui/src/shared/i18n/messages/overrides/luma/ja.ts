import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
  sherlockDx11Performance:
    '起動引数 -dx11 を指定すると、DX11 モードで CPU 性能が低下することがあります。AutoExposure:On で DLAA を使うと、草の縁にジャギーが出ることがあります。',
  guiltyGearStriveAa:
    'キャラクター選択画面ではアンチエイリアスが機能しません。ゲーム内で AA を「Temporal Anti Aliasing」に設定し、Engine.ini に次を追加してください: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4。',
  manualLaunchArgument: 'この起動引数を手動で追加してください。',
  manualEngineIni: '次の設定を Engine.ini に手動で適用してください。',
  publicMatchmaking:
    'Luma のインストール中は公式の公開マッチメイキングを利用しないでください。BAN される可能性があります。',
  edithFinchExit:
    'DLAA は追加の変更なしで動作しますが、終了後にゲームが完全に閉じないことがあります。OptiScaler で解決できる場合があります。',
  dlssNoHdr: 'DLSS のみ（現時点では HDR 非対応）。',
  fallout4DlssGtaoOnly: '現在、このプロファイルがサポートするのは DLSS と GTAO のみです。',
  kh3Txaa: '事前にゲーム内で「TXAA」を選択してください。',
  aceFxaaHigh: '事前にゲーム内で AA を「FXAA High」に選択してください。',
  tetrisFxaa6: '事前にゲーム内で AA を「FXAA:6」、レンダリングスケールを 100% に設定してください。',
  projectWingmanFxaa: '事前にゲーム内で AA を「FXAA」に選択してください。',
  dnfCharacterSelection: 'キャラクター選択画面ではアンチエイリアスが機能しません。',
  tekkenNoD3D9Ex: '前提条件として、起動引数 -nod3d9ex を追加してください。',
  scornOptiscaler:
    'ゲームは FSR 2.1 をネイティブでサポートしているため、OptiScaler で DLSS などのアップスケーラーを追加できます。',
  hatsuneExclusiveFullscreen:
    '不具合がある場合は排他的フルスクリーンを使用しないでください。Alt+Enter で解除できます。',
  deadlineUltra: 'ゲーム設定で「Ultra」を選択してください。',
  filamentAaHigh: 'ゲーム設定で AA を「High」または「Very High」にしてください。',
  aaHigh: 'ゲーム設定で AA を「High」にしてください。',
  aaUltra: 'ゲーム設定で AA を「Ultra」にしてください。',
  mutantMotionBlur:
    'ゲーム設定で AA を「High」にしてください。動きを見やすくするには、Engine.ini で r.motionblur.amount=0 を設定することを推奨します。',
  supralandTaa: 'ゲーム設定で AA を「Temporal Anti Aliasing」にしてください。',
  scarletNexusTxaa: 'ゲーム設定で AA を「TXAA」にしてください。',
  closeToSunAa4x: 'ゲーム設定で AA を 4X にしてください。',
  darksidersAaEpic: 'ゲーム設定で AA を Epic にしてください。',
  codeVeinAaHighest: 'ゲーム設定で AA を Highest にしてください。',
  orcsAaHigh: 'ゲーム設定で AA 品質を「High」にしてください。',
  clashAaVeryHigh: 'ゲーム設定で AA 品質を「Very High」にしてください。',
  vampyrTxaa6x: 'ゲーム設定で AA を TXAA 6X にしてください。',
  crashAaMedium: 'ゲーム設定でアンチエイリアス品質を少なくとも Medium (2x) にしてください。',
  callSeaEpic: 'ゲーム設定で全体品質を「Epic」にしてください。',
  spiritNorthUltra: 'ゲーム設定でグラフィック品質を「Ultra」にしてください。',
  goatHighAa: 'ゲーム設定で High AA を選択してください。',
  crabHighAntialiasing: 'ゲーム設定で High Anti-aliasing Type を選択してください。',
  spyroHighTaa: 'ゲーム設定で High TAA を選択してください。',
  dieYoungTaa: 'ゲーム設定で TAA を「High」または「Epic」にしてください。',
  kakarotBdzKfix:
    'ゲーム設定で TAA を選択し、Legacy 版では BDZKFix、HD 版ではその更新フォークを使用してください。',
  preyData: 'Prey 用の追加 Luma データファイルは、アドオンと同じ場所に置いてください。',
  daymareOptiscalerUuu:
    'Luma は単体では動作しますが、OptiScaler または UUU と併用するとクラッシュします。',
  smtLyallFix: "TAA を強制するには Lyall's Fix が必要です。",
  deusExBorisEnb:
    'Boris ENB（DX9）とは互換性がありません。DE とオリジナル版で動作します。Gold Filter Restoration mod は不要です。',
  xboxStore: 'Xbox Store 版とは互換性がありません。',
  massEffectNativeAa:
    '利用できるのは DLAA / FSR 3 Native AA モードのみで、DLSS または FSR の超解像ではありません。',
  metaphorNativeAa:
    '利用できるのは DLAA / FSR Native AA モードのみで、DLSS または FSR の超解像ではありません。',
  itTakesTwoTitle: 'タイトル画面のシーケンスでのみ動作します。',
  talesAriseSdk: 'UseUE4TAA = true を設定した Arise-SDK が必要です。',
  metroWindowed:
    'ウィンドウまたはボーダーレスウィンドウモードが必要です。MOD を使うか、ゲーム設定ファイルでフルスクリーンを無効にしてください。',
  edithFinch4k:
    'ゲームは 4K で正常に動作しません。Engine.ini を手動で変更する前に Effects を Low に設定してください。',
  sinkingCityOriginal: 'オリジナル版は動作します。リマスター版の状況は不明です。',
  heavyRainSteamUltrawide:
    'ウルトラワイドは Steam 経由で起動した場合にのみ動作することがあります。',
  metroBorderless: 'ボーダーレスウィンドウモードを使用してください。',
  dlssOnlyNoHdr: 'このプロファイルは DLSS 対応のみを追加します。HDR は現在サポートされていません。',
  biomutantAaHighOrMax: 'ゲーム設定で AA を「High」または「Max」にしてください。',
  blairWitchTxaaFull: 'ゲーム設定で TXAA、解像度スケーリングを「Full」にしてください。',
  flickeringIssues: 'ちらつきの問題が発生する可能性があります。',
  brambleEpicVram:
    'Epic 品質では VRAM が徐々に埋まり、カクつきが発生することがあります。Luma の有効中に High と Epic を何度も切り替えないでください。',
  daemonDlaaReset:
    'レベルの読み込みやグラフィック設定の変更により r.TemporalAASamples=1 が強制され、DLAA が無効になります。',
  easyAntiCheatBlocked: 'Easy Anti-Cheat によってブロックされています。',
  echoDlaaAutoExposure:
    '最初のレベル以降は DLAA が機能しません。AutoExposure: On では光源が明滅し、AutoExposure: Off ではアンチエイリアス品質が大幅に低下します。',
  dx11BootFailure: 'DX11 では起動しません。',
  rainCodeAaHighMaxResolution:
    'ゲーム設定で AA 品質を「High」にし、解像度スライダーを最大にしてください。',
  roboquestTaaQuality3: 'ゲーム設定で TAA、Quality を「3」にしてください。',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'ja', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
