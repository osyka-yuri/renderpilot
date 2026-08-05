import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': '算繪預設集',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    '強制使用指定的 DLSS 超解析度預設集。某些遊戲可能還需要設定「強制模型預設集設定檔」，才能套用自訂預設集。',
  'Off (game default)': '停用（遊戲預設值）',
  'Preset A (CNN)': '預設集 A（CNN）',
  'Preset B (CNN)': '預設集 B（CNN）',
  'Preset C (CNN)': '預設集 C（CNN）',
  'Preset D (CNN)': '預設集 D（CNN）',
  'Preset E (CNN)': '預設集 E（CNN）',
  'Preset F (CNN)': '預設集 F（CNN）',
  'Preset G (unused)': '預設集 G（未使用）',
  'Preset H (unused)': '預設集 H（未使用）',
  'Preset I (unused)': '預設集 I（未使用）',
  'Preset J (Transformer Gen 1)': '預設集 J（Transformer Gen 1）',
  'Preset K (Transformer Gen 1)': '預設集 K（Transformer Gen 1）',
  'Preset L (Transformer Gen 2)': '預設集 L（Transformer Gen 2）',
  'Preset M (Transformer Gen 2)': '預設集 M（Transformer Gen 2）',
  'Preset N (unused)': '預設集 N（未使用）',
  'Preset O (unused)': '預設集 O（未使用）',
  Recommended: '建議',
  'Forced Quality Level': '強制品質等級',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    '覆寫遊戲內選取的 DLSS 超解析度品質等級。',
  Performance: '效能',
  Balanced: '平衡',
  Quality: '品質',
  'N/A': '不適用',
  'Ultra Performance': '極致效能',
  Custom: '自訂',
  'Forced Scaling Ratio': '強制縮放比例',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    '設定自訂算繪解析度比例。必須將「強制品質等級」設為「自訂」。',
  Off: '停用',
  '33% (Ultra Performance)': '33%（極致效能）',
  '50% (Performance)': '50%（效能）',
  '58% (Balanced)': '58%（平衡）',
  '67% (Quality)': '67%（品質）',
  '77% (Ultra Quality)': '77%（極致品質）',
  'Enable DLL Override': '啟用 DLL 覆寫',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    '強制遊戲使用系統中安裝的最新 DLSS-SR 版本。大多數支援 DLSS 2 或更新版本的遊戲皆可使用。',
  'On (use latest installed)': '啟用（使用已安裝的最新版本）',
  'Forced Model Preset Profile': '強制模型預設集設定檔',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    '允許在預設情況下「算繪預設集」不生效的遊戲中套用自訂預設集。',
  'Force DLAA (full-resolution)': '強制 DLAA（完整解析度）',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    '以完整解析度算繪每個 DLSS 品質模式，使其作為反鋸齒方案（DLAA）運作。',
  On: '啟用',
  'Remap Performance to Ultra Performance': '將「效能」重新對應至「極致效能」',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    '強制「效能」品質模式使用「極致效能」算繪路徑。',
  'Forces a specific DLSS Frame Generation preset.': '強制使用指定的 DLSS 畫格生成預設集。',
  'Preset A': '預設集 A',
  'Preset B': '預設集 B',
  'Preset C (unused)': '預設集 C（未使用）',
  'Preset D (unused)': '預設集 D（未使用）',
  'Preset E (unused)': '預設集 E（未使用）',
  'Preset F (unused)': '預設集 F（未使用）',
  'Preset J (unused)': '預設集 J（未使用）',
  'Preset K (unused)': '預設集 K（未使用）',
  'Preset L (unused)': '預設集 L（未使用）',
  'Preset M (unused)': '預設集 M（未使用）',
  'Forced Mode': '強制模式',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    '設定畫格生成模式。動態模式需要 595.97 或更新版本的驅動程式。',
  Fixed: '固定',
  Dynamic: '動態',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    '強制遊戲使用系統中安裝的最新 DLSS-FG 版本。大多數 DLSS 3 遊戲皆可使用。',
  'Multi-Frame Generation — Fixed Count': '多畫格生成 — 固定數量',
  'Sets a fixed number of generated frames per rendered frame.':
    '設定每個算繪畫格所對應的固定生成畫格數。',
  'Multi-Frame Generation — Dynamic Count': '多畫格生成 — 動態數量',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    '設定畫格生成處於動態模式時的生成畫格數上限。',
  'Up to 2x': '最高 2x',
  'Up to 3x': '最高 3x',
  'Up to 4x': '最高 4x',
  'Up to 5x': '最高 5x',
  'Up to 6x': '最高 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate': '多畫格生成 — 目標動態畫格率',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    '設定動態畫格生成所要維持的目標畫格率。',
  'Max Refresh Rate': '最高更新率',
  'Forces a specific DLSS Ray Reconstruction preset.': '強制使用指定的 DLSS 光線重建預設集。',
  'Preset D (Transformer Gen 1)': '預設集 D（Transformer Gen 1）',
  'Preset E (Transformer Gen 1)': '預設集 E（Transformer Gen 1）',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    '覆寫遊戲內選取的 DLSS 光線重建品質等級。',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    '強制遊戲使用系統中安裝的最新 DLSS-RR 版本。大多數支援光線重建的遊戲皆可使用。',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'zh-Hant', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
