import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': 'レンダリングプリセット',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    '特定の DLSS 超解像度プリセットを強制します。ゲームによっては、カスタムプリセットを適用するために「モデルプリセットプロファイルを強制」も必要です。',
  'Off (game default)': 'オフ（ゲームの既定値）',
  'Preset A (CNN)': 'プリセット A（CNN）',
  'Preset B (CNN)': 'プリセット B（CNN）',
  'Preset C (CNN)': 'プリセット C（CNN）',
  'Preset D (CNN)': 'プリセット D（CNN）',
  'Preset E (CNN)': 'プリセット E（CNN）',
  'Preset F (CNN)': 'プリセット F（CNN）',
  'Preset G (unused)': 'プリセット G（未使用）',
  'Preset H (unused)': 'プリセット H（未使用）',
  'Preset I (unused)': 'プリセット I（未使用）',
  'Preset J (Transformer Gen 1)': 'プリセット J（Transformer Gen 1）',
  'Preset K (Transformer Gen 1)': 'プリセット K（Transformer Gen 1）',
  'Preset L (Transformer Gen 2)': 'プリセット L（Transformer Gen 2）',
  'Preset M (Transformer Gen 2)': 'プリセット M（Transformer Gen 2）',
  'Preset N (unused)': 'プリセット N（未使用）',
  'Preset O (unused)': 'プリセット O（未使用）',
  Recommended: '推奨',
  'Forced Quality Level': '品質レベルを強制',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    'ゲーム内で選択された DLSS 超解像度の品質レベルを上書きします。',
  Performance: 'パフォーマンス',
  Balanced: 'バランス',
  Quality: 'クオリティ',
  'N/A': '該当なし',
  'Ultra Performance': 'ウルトラパフォーマンス',
  Custom: 'カスタム',
  'Forced Scaling Ratio': 'スケーリング比率を強制',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    'カスタムのレンダリング解像度比率を設定します。「品質レベルを強制」を「カスタム」に設定する必要があります。',
  Off: 'オフ',
  '33% (Ultra Performance)': '33%（ウルトラパフォーマンス）',
  '50% (Performance)': '50%（パフォーマンス）',
  '58% (Balanced)': '58%（バランス）',
  '67% (Quality)': '67%（クオリティ）',
  '77% (Ultra Quality)': '77%（ウルトラクオリティ）',
  'Enable DLL Override': 'DLL オーバーライドを有効化',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    'システム全体にインストールされている最新の DLSS-SR バージョンをゲームに強制します。DLSS 2 以降のほとんどのタイトルでサポートされています。',
  'On (use latest installed)': 'オン（インストール済みの最新版を使用）',
  'Forced Model Preset Profile': 'モデルプリセットプロファイルを強制',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    '「レンダリングプリセット」が既定では反映されないゲームで、カスタムプリセットを適用できるようにします。',
  'Force DLAA (full-resolution)': 'DLAA を強制（フル解像度）',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    'すべての DLSS 品質モードをフル解像度でレンダリングし、アンチエイリアス機能（DLAA）として動作させます。',
  On: 'オン',
  'Remap Performance to Ultra Performance': 'パフォーマンスをウルトラパフォーマンスに置き換え',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    '品質モード「パフォーマンス」に「ウルトラパフォーマンス」のレンダリング経路を強制します。',
  'Forces a specific DLSS Frame Generation preset.':
    '特定の DLSS フレーム生成プリセットを強制します。',
  'Preset A': 'プリセット A',
  'Preset B': 'プリセット B',
  'Preset C (unused)': 'プリセット C（未使用）',
  'Preset D (unused)': 'プリセット D（未使用）',
  'Preset E (unused)': 'プリセット E（未使用）',
  'Preset F (unused)': 'プリセット F（未使用）',
  'Preset J (unused)': 'プリセット J（未使用）',
  'Preset K (unused)': 'プリセット K（未使用）',
  'Preset L (unused)': 'プリセット L（未使用）',
  'Preset M (unused)': 'プリセット M（未使用）',
  'Forced Mode': 'モードを強制',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    'フレーム生成モードを設定します。ダイナミックモードにはドライバー 595.97 以降が必要です。',
  Fixed: '固定',
  Dynamic: 'ダイナミック',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    'システム全体にインストールされている最新の DLSS-FG バージョンをゲームに強制します。ほとんどの DLSS 3 タイトルでサポートされています。',
  'Multi-Frame Generation — Fixed Count': 'マルチフレーム生成 — 固定数',
  'Sets a fixed number of generated frames per rendered frame.':
    'レンダリングされたフレームごとに生成するフレーム数を固定します。',
  'Multi-Frame Generation — Dynamic Count': 'マルチフレーム生成 — 動的数',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    'フレーム生成がダイナミックモードのときに、生成フレーム数の上限を設定します。',
  'Up to 2x': '最大 2x',
  'Up to 3x': '最大 3x',
  'Up to 4x': '最大 4x',
  'Up to 5x': '最大 5x',
  'Up to 6x': '最大 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate':
    'マルチフレーム生成 — 動的ターゲットフレームレート',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    'ダイナミックフレーム生成が維持するターゲットフレームレートを設定します。',
  'Max Refresh Rate': '最大リフレッシュレート',
  'Forces a specific DLSS Ray Reconstruction preset.':
    '特定の DLSS レイ再構成プリセットを強制します。',
  'Preset D (Transformer Gen 1)': 'プリセット D（Transformer Gen 1）',
  'Preset E (Transformer Gen 1)': 'プリセット E（Transformer Gen 1）',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    'ゲーム内で選択された DLSS レイ再構成の品質レベルを上書きします。',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    'システム全体にインストールされている最新の DLSS-RR バージョンをゲームに強制します。レイ再構成対応タイトルのほとんどでサポートされています。',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'ja', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
