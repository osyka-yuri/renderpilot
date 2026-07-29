import type { MessageKey } from './en';
import type { MessageValue } from './types';

export const ja: Record<MessageKey, MessageValue> = {
  'nav.games': 'ゲーム',
  'nav.libraries': 'ライブラリ',
  'nav.settings': '設定',
  'nav.operations': '操作履歴',
  'nav.gameFallback': 'ゲーム',
  'nav.donate': '寄付する',
  'shell.refresh': '更新',
  'shell.updateAvailable': 'アップデートがあります',

  'settings.appearance.title': '外観',
  'settings.appearance.description': 'アプリケーションの見た目と言語をカスタマイズします。',
  'settings.appearance.theme.title': 'テーマ',
  'settings.appearance.theme.description': 'アプリケーションのカラーテーマを選択します。',
  'settings.appearance.theme.triggerLabel': 'テーマ',
  'settings.appearance.language.title': '言語',
  'settings.appearance.language.description': 'インターフェース言語を選択します。',
  'settings.appearance.language.triggerLabel': '言語',
  'settings.appearance.language.placeholder': '言語を選択',

  'settings.theme.system': 'システム',
  'settings.theme.dark': 'ダーク',
  'settings.theme.light': 'ライト',

  'settings.language.system': 'システムデフォルト',
  'settings.language.en': 'English',
  'settings.language.ru': 'Русский',
  'settings.language.es': 'Español',
  'settings.language.zh': '中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  'settings.tabs.general': '一般',
  'settings.tabs.renodx': 'RenoDX',
  'settings.tabs.catalog': 'カタログ',
  'settings.tabs.nvidia': 'NVIDIA',

  // ── Settings: NVIDIA ──
  'settings.nvidia.indicator.title': 'DLSS インジケーター',
  'settings.nvidia.indicator.description':
    'ゲームプレイ中に、アクティブな DLSS バージョンと設定を含むオーバーレイを表示します。',
  'settings.nvidia.indicator.systemWide': 'システム全体',
  'settings.nvidia.indicator.adminRequired':
    'この設定を変更するには、アプリを管理者として再起動してください。',
  'settings.nvidia.indicator.overlayTitle': '画面オーバーレイ',
  'settings.nvidia.indicator.overlayDescription': 'この PC 上のすべてのゲームに適用されます。',
  'settings.nvidia.indicator.toggleAria': 'DLSS インジケーターの切り替え',
  'settings.nvidia.global.title': 'グローバル DLSS 設定',
  'settings.nvidia.global.description':
    'NVIDIA ベースプロファイルを介して、ゲーム固有の上書きがないすべてのゲームに適用される既定値です。',
  'settings.nvidia.global.systemWide': 'システム全体',
  'settings.nvidia.global.adminRequired':
    'これらの設定を変更するには、管理者としてアプリを再起動してください。',
  'settings.nvidia.global.familySr': 'DLSS スーパーレゾリューション',
  'settings.nvidia.global.familyFg': 'DLSS フレーム生成',
  'settings.nvidia.global.familyRr': 'DLSS レイ再構築',
  'settings.nvidia.unsupported.title': 'NVIDIA GPU が検出されませんでした',
  'settings.nvidia.unsupported.description':
    'これらの設定には対応する NVIDIA グラフィックスカードが必要です。',

  'game.card.action.details': '詳細',
  'game.card.action.detailsAria': '{title} の詳細を開く',
  'game.card.detectedLibraries': '検出されたコンポーネント',
  'game.card.availableAddons': '利用可能なアドオン',
  'game.card.badge.upToDate': '最新',
  'game.card.badge.updatesAvailable': '利用可能なアップデート',
  'game.card.badge.updatesAvailableCount': {
    one: '1件のアップデートが利用可能',
    other: '{count}件のアップデートが利用可能',
  },
  'game.card.menu.ariaLabel': '{title} のオプション',
  'game.card.menu.favorite.add': 'お気に入りに追加',
  'game.card.menu.favorite.remove': 'お気に入りから削除',
  'game.card.menu.favorite.toggleHint': 'このゲームのお気に入り状態を切り替えます。',
  'game.card.menu.hidden.add': 'ゲームを隠す',
  'game.card.menu.hidden.remove': 'ゲームの非表示を解除',
  'game.card.menu.hidden.toggleHint': 'このゲームの非表示状態を切り替えます。',
  'game.card.menu.removeFromCatalog': 'カタログから削除',
  'game.card.menu.removeFromCatalogHint': '手動で追加したこのゲームをカタログから除外します。',
  'game.card.removeConfirm.title': '{title} をカタログから削除しますか？',
  'game.card.removeConfirm.description':
    'RenderPilot は管理対象の変更を安全に元に戻してから、カードと関連履歴を削除します。ゲーム自体のファイルは変更されません。',
  'game.card.removeConfirm.action': 'カタログから削除',

  'game.cover.alt': 'カバー画像',
  'game.cover.altWithTitle': 'カバー画像：{title}',
  'game.cover.menu.fetch': 'カバー画像をダウンロード',
  'game.cover.menu.fetching': 'ダウンロード中…',
  'game.cover.menu.fetchHint': 'オンラインでカバー画像を検索します。',
  'game.cover.menu.pick': '画像ファイルを選択…',
  'game.cover.menu.pickHint': 'ローカルの画像をカバー画像として選択します。',
  'game.cover.menu.clear': 'カバー画像を削除',
  'game.cover.menu.clearHint': 'デフォルトのカバー画像に戻します。',

  'game.dashboard.summary': 'ダッシュボード',
  'game.dashboard.games': { one: '{count} 個のゲーム', other: '{count} 個のゲーム' },
  'game.dashboard.updates': { one: '{count} 件のアップデート', other: '{count} 件のアップデート' },

  'elevation.title': '管理者権限が必要です',
  'elevation.description': '一部の設定は管理者権限なしでは変更できません。',
  'elevation.relaunch': '管理者として再起動',
  'elevation.relaunchFailed': '管理者として再起動できませんでした',
  'elevation.dismiss': '閉じる',
  'error.boundary.title': '問題が発生しました',
  'error.boundary.description':
    'この画面で予期しないエラーが発生しました。もう一度試すか、別のセクションに切り替えてください。',
  'error.boundary.reset': '再試行',
  'pageLoad.loading': 'ページを読み込んでいます…',
  'pageLoad.error.title': 'このページを開けませんでした',
  'pageLoad.error.description':
    'ページを読み込めませんでした。もう一度試すか、ゲーム一覧に戻ってください。',
  'pageLoad.error.retry': '再試行',
  'pageLoad.error.backToGames': 'ゲーム一覧に戻る',

  'games.addGame': 'ゲームを追加',
  'games.addingGame': 'ゲームを追加中...',
  'games.chooseInstallFolder': 'ゲームのインストールフォルダーを選択',
  'addGame.title': 'ゲームを追加',
  'addGame.cannotAddTitle': 'ゲームを追加できませんでした',
  'addGame.installRoot': 'インストールルート',
  'addGame.reviewTitle': 'ゲームのインストールを確認',
  'addGame.reviewDescription': '1つのゲームを追加する前にインストールルートを確認してください。',
  'addGame.selectedFolder': '選択したフォルダー',
  'addGame.recommendedFolder': '推奨インストールルート',
  'addGame.existingRoot': '現在のゲームフォルダー',
  'addGame.chooseExecutable': 'ゲーム実行ファイル',
  'addGame.chooseExecutablePlaceholder': '実行ファイルを選択',
  'addGame.chooseAnother': '選び直す',
  'addGame.add': 'ゲームを追加',
  'addGame.addSelected': '選択したフォルダーを追加',
  'addGame.correctRoot': 'パスを修正',
  'addGame.addRecommended': '推奨ルートを追加',
  'addGame.replaceRootTitle': 'ゲームのパスを修正',
  'addGame.replaceRootDescription':
    'RenderPilot は現在のフォルダーの代わりに、選択したフォルダーを使用します。ゲームファイルは変更されません。',
  'addGame.replaceExistingRoot': 'パスを修正',
  'addGame.rootCorrection.rollbackTitle': '先に有効なコンポーネント変更を元に戻す必要があります',
  'addGame.rootCorrection.rollbackDescription':
    'カードのルートを置き換える前に、{count} 件のコンポーネント変更を元に戻す必要があります。',
  'addGame.rootCorrection.rollbackAndReplace': '変更を元に戻してルートを置き換える',
  'addGame.rootCorrection.rollbackFailed':
    'コンポーネントの変更を完全には元に戻せませんでした。現在のゲームルートは変更されていません。',
  'addGame.rootCorrection.blocker.pendingRecovery': '中断されたファイル操作の復旧がまだ必要です。',
  'addGame.rootCorrection.blocker.installedAddon':
    'インストール済みアドオンが選択したゲームフォルダー外のファイルに属しています。',
  'addGame.rootCorrection.blocker.nvapi':
    '有効な NVIDIA プロファイル設定が選択したフォルダー外の実行ファイルに属しています。',
  'addGame.rootCorrection.blocker.orphanedComponentBaseline':
    '保存済みのロールバック状態に対応するコンポーネントがありません。',
  'addGame.rescan': 'ゲームを再スキャン',
  'addGame.catalogBusy': '別のカタログ操作が実行中です。完了してからもう一度お試しください。',
  'addGame.warning.legacyCardsConsolidated':
    '誤って作成されたことが確認された従来のゲームカードを {count} 件統合しました。',
  'addGame.warning.legacyCardsRetained':
    '独立したインストールかどうかを判断できないため、従来のゲームカードを {count} 件保持しました。',
  'addGame.warning.recoveryBundleCreated':
    '競合する従来の状態を復旧パッケージ {path} に保存しました。',
  'addGame.warning.rootCorrectionHistoryArchived':
    '修正後のゲームルート外にあるカタログ履歴を復旧パッケージ {path} に保存しました。',
  'addGame.warning.recoveryBundleFallback': '復旧パッケージ: {path}',
  'addGame.warning.unsupportedPlatform':
    'ゲームインストールの検査は Windows でのみサポートされています。',
  'addGame.warning.probeIncomplete':
    '一部のフォルダーを検査できなかったため、推奨結果の信頼度が下がっています。',
  'addGame.warning.parentProbeIncomplete':
    '推奨された親フォルダーを完全に検査できませんでした。追加する前に確認してください。',
  'addGame.unavailable.multipleInstalls':
    '選択したフォルダーは、複数のゲームを含む共通ライブラリのようです。個別のゲームフォルダーを選択してください。',
  'addGame.unavailable.containsProvenInstall':
    '選択したフォルダー内に、すでに認識されているゲームのインストールがあります。共通の親フォルダーではなく、そのゲームの正確なフォルダーを選択してください。',
  'addGame.unavailable.containsMultipleCatalogInstalls':
    '選択したフォルダー内に、すでに認識されているゲームが複数あります。個別のゲームフォルダーを選択してください。',
  'addGame.unavailable.insideExistingInstall':
    '選択したフォルダーは、追加済みゲームのインストール内にあります。そのゲームのインストールルートを使用してください。',
  'addGame.unavailable.noReadableExecutable':
    '選択したフォルダーに読み取り可能なゲーム実行ファイルが見つかりません。ゲーム実行ファイルを含むインストールフォルダーを選択してください。',
  'addGame.unavailable.rootCorrectionBlocked':
    '管理対象の状態が残っているため、既存のインストールルートを安全に変更できません。先に表示されたブロック要因を解消してください。',
  'addGame.warning.insideExistingInstall':
    'このフォルダーは既存のゲームに属しています。そのゲームのインストールルートを使用してください。',
  'addGame.warning.narrowsExistingInstall':
    '既存の手動ルートには複数のゲームフォルダーが含まれている可能性があります。確定すると同じカードを維持したまま、ルートを選択したフォルダーに修正します。',
  'addGame.warning.multipleProvenInstalls':
    'このフォルダーには、確認済みのゲームインストールが複数含まれています。',
  'addGame.warning.containsProvenInstall':
    'このフォルダーには確認済みのゲームインストールが含まれています。正確なルートを使用してください。',
  'addGame.warning.multipleInstallsSuspected':
    '別々の子フォルダーにある実行ファイルは、異なるゲームに属している可能性があります。確定した場合でも、このフォルダーは 1 つのゲームとして扱われます。',
  'addGame.warning.explicitExecutableRequired':
    '有効な実行ファイルがすべてランチャーまたは補助ツールに見えます。使用するファイルを明示的に選択してください。',
  'addGame.warning.noReadableExecutable':
    '読み取り可能なゲーム実行ファイルがないため、このフォルダーを単独で追加することはできません。',
  'addGame.warning.filesystemProbeError':
    'インストールの一部を検査できませんでした。ファイルのアクセス権限を確認してください。',
  'games.libraryActions': 'アクション',
  'games.search': 'ゲームを検索',
  'games.openFilters': 'フィルター',
  'games.openFiltersActive': 'フィルター（適用中）',
  'games.loading': '読み込み中...',
  'games.empty.title': 'ゲームが見つかりません',
  'games.empty.description': 'ゲームを追加するとダッシュボードに表示されます。',
  'games.filterEmpty.title': '一致する結果がありません',
  'games.filterEmpty.description': '検索条件またはフィルターを変更してみてください。',
  'games.filterEmpty.reset': 'フィルターをリセット',

  'settings.catalog.title': 'カバー画像のソース',
  'settings.catalog.description':
    'ゲームのカバー画像をダウンロードするオンラインソースを選択します。',
  'settings.catalog.steamKey.srLabel': 'SteamGridDB API キー',
  'settings.catalog.steamKey.placeholder': 'API キー',
  'settings.catalog.steamKey.loading': '読み込み中…',
  'settings.catalog.steamKey.save': '保存',
  'settings.catalog.steamKey.saved': '保存しました',
  'settings.catalog.steamKey.cleared': 'クリアしました',
  'settings.catalog.steamKey.readError': '設定の読み込みに失敗しました。',
  'settings.catalog.steamKey.saveError': '設定の保存に失敗しました。',
  'settings.catalog.steamKey.show': 'API キーを表示',
  'settings.catalog.steamKey.hide': 'API キーを非表示',
  'settings.catalog.steamKey.getKey': 'API キーを取得',

  'settings.renodx.vulkan.description':
    'Vulkan RenoDX ゲームで使う共有 ReShade Vulkan レイヤーを管理します。',
  'settings.renodx.vulkan.channel': 'Vulkan レイヤーチャネル',
  'settings.renodx.vulkan.channelDescription':
    '共有 Vulkan レイヤーで使う ReShade チャネルを選択します。',
  'settings.renodx.vulkan.loadError': 'Vulkan レイヤーの状態を読み込めませんでした。',
  'settings.renodx.vulkan.saveError': 'Vulkan レイヤーチャネルを保存できませんでした。',
  'settings.renodx.vulkan.applyError': 'Vulkan レイヤーを適用できませんでした。',

  'common.unknown': '不明',
  'common.downloadProgress': 'ダウンロードの進行状況',

  'gameDetails.noGameSelected.title': 'ゲームが選択されていません',
  'gameDetails.noGameSelected.description':
    '詳細を表示するには、ダッシュボードからゲームを選択してください。',

  'gameDetails.version.noReplacements': '代替バージョンなし',
  'gameDetails.version.restoreOriginal': '元の {fileName} を復元',
  'gameDetails.version.fileCount': { one: '1 個のファイル', other: '{count} 個のファイル' },

  'gameDetails.vendor.description': 'コンポーネントのバージョンを変更します。',

  'gameDetails.dlss.description': 'DLSS バージョンを変更するか、設定を上書きします。',
  'gameDetails.dlss.descriptionSwapOnly': 'DLSS バージョンを変更します。',
  'gameDetails.dlss.libraryFileLabel': 'ファイルバージョン',
  'gameDetails.dlss.driverOverridesLabel': 'NVIDIA プロファイルの上書き',
  'gameDetails.dlss.adminRequired':
    'これらの設定を変更するには、アプリを管理者として再起動してください。',

  'gameDetails.streamline.description': 'Streamline プラグインを管理します。',
  'gameDetails.streamline.versionTitle': 'グローバル Streamline バージョン',
  'gameDetails.streamline.versionDescription': 'すべてのプラグインに同じバージョンを適用します。',
  'gameDetails.streamline.noOtherVersions': '他のバージョンはありません',
  'gameDetails.streamline.mixed': '混在するバージョン',
  'gameDetails.streamline.mixedRange': '混在するバージョン (v{min} – v{max})',
  'gameDetails.streamline.updatesSummary': '{updates} 件のアップデート · {missing} 個の不足',
  'gameDetails.streamline.restoreAllAria': 'すべてのプラグインを元の状態に復元',
  'gameDetails.streamline.restoreAllTooltip': 'すべて元の状態に復元',
  'gameDetails.updateAll.action': 'すべて更新',
  'gameDetails.updateAll.actionCount': 'すべて更新 ({count})',
  'gameDetails.updateAll.upToDate': 'すべての安定版は最新です',
  'gameDetails.updateAll.partialFailure':
    '一部の更新に失敗しました（{count}）。詳細を確認して再試行してください。',
  'gameDetails.updateAll.tooltip': {
    one: '{count} 個のコンポーネントを最新の安定版に更新します',
    other: '{count} 個のコンポーネントを最新の安定版に更新します',
  },
  'gameDetails.executable.title': 'ゲームの実行ファイル',
  'gameDetails.developerMode.requiredTitle': 'Windows の開発者モードが無効です',
  'gameDetails.developerMode.requiredDescription':
    'Microsoft D3D12 Agility Preview には、この Windows 設定が必要です。',
  'gameDetails.developerMode.checkTitle': '開発者モードを確認できませんでした',
  'gameDetails.developerMode.checkDescription':
    'RenderPilot は Windows の開発者モードの現在の状態を確認できませんでした。',
  'gameDetails.developerMode.checkUnavailable':
    '続行するには、状態の確認に成功する必要があります。',
  'gameDetails.developerMode.enableGuidance':
    '開発者モードは、Windows の設定にある「開発者向け」で有効にできます。',
  'gameDetails.developerMode.previewGuidance':
    'Windows で開発者モードを有効にする方法は、Microsoft のドキュメントで確認できます。',
  'gameDetails.developerMode.restartInfo': 'Windows の再起動後に設定が反映される場合があります。',
  'gameDetails.developerMode.stillDisabled':
    '開発者モードはまだ無効です。最近有効にした場合は、変更を反映するために Windows の再起動が必要になることがあります。',
  'gameDetails.developerMode.settingsOpenFailed':
    'Windows の設定を開けませんでした。「開発者向け」を手動で開いてください。',
  'gameDetails.developerMode.documentationOpenFailed':
    'Microsoft のドキュメントを開けませんでした。',
  'gameDetails.developerMode.openSettings': '設定を開く',
  'gameDetails.developerMode.openDocumentation': 'ドキュメントを開く',
  'gameDetails.developerMode.checkStatus': '状態を確認',
  'gameDetails.developerMode.retryCheck': '確認を再試行',
  'gameDetails.developerMode.checkingStatus': '確認中…',
  'gameDetails.d3d12.status.original': '元の EXE',
  'gameDetails.d3d12.status.patched': 'EXE パッチ済み: {from} → {to}',
  'gameDetails.d3d12.status.repair': '修復が必要です',
  'gameDetails.d3d12.repairGuidance':
    'ゲームファイルを確認して再スキャンしてください。RenderPilot はこの EXE を上書きしません。',
  'gameDetails.d3d12.action.patch': 'EXE をパッチ: {from} → {to}',
  'gameDetails.d3d12.action.restore': 'EXE を復元: {from} → {to}',
  'gameDetails.d3d12.action.repair': '先に EXE の修復が必要です',
  'gameDetails.d3d12.action.blocked': '現在の状態では、この D3D12 バージョンを適用できません。',
  'gameDetails.d3d12.action.planPatch': 'パッチを適用します: SDK {from} → {to}',
  'gameDetails.d3d12.action.planRestore': '元の EXE を復元します: SDK {from} → {to}',
  'gameDetails.d3d12.select.compatible': '現在の EXE と互換',
  'gameDetails.d3d12.select.changesExecutable': 'EXE の変更が必要',
  'gameDetails.d3d12.select.unavailable': '利用不可',
  'gameDetails.d3d12.confirm.title': 'EXE の変更を確認',
  'gameDetails.d3d12.confirm.description':
    'RenderPilot は実行ファイルの D3D12SDKVersion エクスポートを変更します。',
  'gameDetails.d3d12.confirm.updateAllDescription':
    'これらの更新では、表示されたゲーム EXE の D3D12 SDK ラインを切り替える必要があります。確認するまでダウンロードも変更も行いません。',
  'gameDetails.d3d12.confirm.backup': 'バックアップ先: {path}',
  'gameDetails.d3d12.confirm.backupWillCreate':
    '変更前に元の EXE のバックアップを次の場所へ作成します: {path}',
  'gameDetails.d3d12.confirm.backupExists':
    '元の EXE はすでに次の場所へ保存されています: {path}。このコピーは上書きされません。',
  'gameDetails.d3d12.confirm.signatureWarning':
    '変更後は EXE のデジタル署名が無効と判断され、整合性チェックでファイルの変更が検出される場合があります。D3D12 を完全にロールバックすると、RenderPilot が元の EXE を復元します。',
  'gameDetails.d3d12.confirm.accept': '変更',
  'gameDetails.d3d12.executableLockedTitle': 'EXE の選択はロックされています',
  'gameDetails.d3d12.executableLocked':
    '別の EXE を選択するには、D3D12 コンポーネントを完全にロールバックしてください。',
  'gameDetails.d3d12.executableRepairLocked':
    'D3D12 カードの手順に従って復旧してから、ゲームを再スキャンしてください。',
  'gameDetails.executable.description':
    'ゲームの実行ファイル — NVIDIA プロファイルが適用され、RenoDX はそのフォルダーにインストールされます。',
  'gameDetails.executable.triggerAria': 'ゲーム実行ファイル: {fileName}',
  'gameDetails.executable.detectedGroup': '検出されたゲーム実行ファイル',
  'gameDetails.executable.otherGroup': 'その他（ランチャー・インストーラー・ツール）',
  'gameDetails.executable.customBadge': '手動',
  'gameDetails.executable.reset': '自動検出に戻す',
  'gameDetails.executable.tooltipAuto':
    'ゲームの実行ファイル：自動検出。NVIDIA プロファイルと RenoDX で使用されます。',
  'gameDetails.executable.tooltipCustom':
    'ゲームの実行ファイル：手動で選択。NVIDIA プロファイルと RenoDX で使用されます。',
  'gameDetails.profile.title': 'NVIDIA プロファイル',
  'gameDetails.profile.description': 'このゲームの NVIDIA ドライバ設定を構成します。',
  'gameDetails.profile.pinnedManual': '手動で選択済み。',
  'gameDetails.profile.autoDetected': '自動検出済み。',
  'gameDetails.profile.noExeDetected': 'このゲームの実行可能ファイルが見つかりません。',
  'gameDetails.profile.noExe': '実行ファイルなし',
  'gameDetails.profile.noProfile': 'NVIDIA プロファイルが見つかりません。',

  'gameDetails.nvapi.requiresDriver': 'ドライバー {version}+ が必要です',
  'gameDetails.nvapi.unavailable': '利用不可',
  'gameDetails.nvapi.resetDefault': 'デフォルトにリセット',
  'gameDetails.nvapi.alreadyDefault': '既にデフォルトです',
  'gameDetails.nvapi.restoreBaselineAria': '初期値に戻す',
  'gameDetails.nvapi.restoreBaseline': '初期値に戻す',
  'gameDetails.nvapi.alreadyBaseline': '既に初期値です',
  'gameDetails.nvapi.noBaseline': '初期値が保存されていません',

  'gameDetails.nvapi.warning.noDll': 'インストールディレクトリに DLSS DLL が見つかりません。',
  'gameDetails.nvapi.warning.noManifest': 'マニフェストにこの DLL バージョンのデータがありません。',
  'gameDetails.nvapi.warning.noExecutable': 'このゲームの実行可能ファイルが見つかりません。',
  'gameDetails.nvapi.warning.nvapiUnavailable': 'NVAPI が利用できません。',
  'gameDetails.nvapi.warning.nvapiInitFailed': 'NVAPI の初期化に失敗しました。',
  'gameDetails.nvapi.warning.drsFailed': 'DRS セッションを作成できませんでした。',

  'operations.title': '操作履歴',
  'operations.subtitleGame': '{title} のアクティビティ',
  'operations.loading': '読み込み中...',
  'operations.empty': '履歴はまだありません',
  'operations.gameName': 'ゲーム',
  'operations.date': '日付',
  'operations.status': 'ステータス',
  'operations.action': 'アクション',
  'operations.libraryType': 'ライブラリタイプ',
  'operations.version': 'バージョン',

  'libraries.error': 'エラー',
  'libraries.catalogFallback.title': 'カタログを利用できません',
  'libraries.catalogFallback.description':
    'ローカルに登録されたパッケージのみを表示しています。完全なカタログではありません。',
  'libraries.state.localOnly': 'ローカルのみ',
  'libraries.state.downloaded': 'ダウンロード済み',
  'libraries.state.missing': 'ファイルがありません',
  'libraries.state.corrupt': 'ファイルが破損しています',
  'libraries.hash.copy': 'ハッシュをコピー',
  'libraries.hash.copied': 'コピーしました',
  'libraries.hash.failed': 'コピーに失敗しました',
  'libraries.hash.copiedToast': 'ハッシュをクリップボードにコピーしました',
  'libraries.sort.asc': '昇順',
  'libraries.sort.desc': '降順',
  'libraries.sort.none': '並べ替えなし',
  'libraries.actions.delete': '削除',
  'libraries.actions.download': 'ダウンロード',
  'libraries.actions.deletedToast': '{version} を削除しました',
  'libraries.actions.downloadedToast': '{version} をダウンロードしました',
  'libraries.actions.failedToast': '{action} に失敗しました',
  'libraries.actions.downloadAll': '最新をダウンロード',
  'libraries.actions.downloadAllCount': '最新をダウンロード（{count}）',
  'libraries.actions.downloadAllUpToDate': 'すべての最新バージョンはダウンロード済みです',
  'libraries.actions.downloadAllTooltip': {
    one: '{count} 個の最新バージョンをダウンロード',
    other: '{count} 個の最新バージョンをダウンロード',
  },
  'libraries.actions.downloadAllDoneToast': {
    one: '{count} 個のライブラリをダウンロードしました',
    other: '{count} 個のライブラリをダウンロードしました',
  },
  'libraries.actions.downloadAllPartialToast': '{succeeded} 個ダウンロード、{failed} 個失敗',
  'libraries.actions.downloadAllNoneToast': 'すべての最新バージョンはダウンロード済みです',

  'common.cancel': 'キャンセル',
  'common.apply': '適用',

  'filters.title': 'フィルター',
  'filters.launchers.title': 'ランチャー',
  'filters.launchers.empty': 'ランチャーが見つかりません',
  'filters.launchers.reorder': '{label} を移動',
  'filters.libraries.title': 'コンポーネント',
  'filters.libraries.empty': 'コンポーネントが見つかりません',
  'filters.addons.title': 'アドオン',

  'games.favoritesToggle': 'お気に入り',
  'games.favoritesToggleActive': 'お気に入り（有効）',
  'games.showHiddenActive': '非表示のゲーム（有効）',
  'games.showHidden': '表示',

  'operation.label.low': '低リスク',
  'operation.label.medium': '中リスク',
  'operation.label.high': '高リスク',
  'operation.label.blocked': 'ブロックされました',
  'operation.label.planned': '計画済み',
  'operation.label.completed': '完了',
  'operation.label.failed': '失敗',
  'operation.label.rolledBack': 'ロールバック済み',
  'operation.label.replaceComponent': 'バージョン変更',
  'operation.duration': '{seconds}秒で完了',
  'operation.filesUpdated.none': '更新されたファイルはありません。',
  'operation.filesUpdated.count': {
    one: '1個のファイルが更新されました。',
    other: '{count}個のファイルが更新されました。',
  },
  'operation.filesRestored.none': '復元されたファイルはありません。',
  'operation.filesRestored.count': {
    one: '1個のファイルが復元されました。',
    other: '{count}個のファイルが復元されました。',
  },
  'operation.itemAria': '{kind}、{status}',

  'notify.stalePlan': '操作プランが古くなっています。再試行してください。',
  'notify.missingStableGameId': 'ゲームを特定できませんでした。',
  'notify.coverPickerPreview': 'カバー画像を選択するには、デスクトップアプリを使用してください。',
  'notify.coverUpdated.title': 'カバー画像を更新しました',
  'notify.coverUpdated.body': 'カスタムカバー画像が保存されました。',
  'notify.coverDownloaded.title': 'カバー画像をダウンロードしました',
  'notify.coverDownloaded.body': 'ゲームのカバー画像が更新されました。',
  'notify.coverRemoved.title': 'カバー画像を削除しました',
  'notify.coverRemoved.body': 'デフォルトのカバー画像に戻しました。',
  'notify.favoriteFailed': 'お気に入りの状態を変更できませんでした。',
  'notify.favoriteAdded': 'お気に入りに追加しました。',
  'notify.favoriteRemoved': 'お気に入りから削除しました。',
  'notify.hiddenFailed': '非表示の状態を変更できませんでした。',
  'notify.gameHidden': 'ゲームを非表示にしました。',
  'notify.gameUnhidden': 'ゲームを表示にしました。',
  'notify.gameRemovedFromCatalog': 'ゲームをカタログから削除しました。',
  'notify.removeGameFailed': 'ゲームをカタログから削除できませんでした。',
  'notify.applyCompleted': '変更を適用しました',
  'notify.rollbackCompleted': 'ロールバックが完了しました',
  'notify.swapBatchFailed.title': '一部の更新に失敗しました',
  'notify.swapBatchFailed.description':
    '{total} 個中 {failed} 個のコンポーネントの更新に失敗しました。',
  'notify.rollbackBatchFailed.title': '一部の復元に失敗しました',
  'notify.rollbackBatchFailed.description':
    '{total} 個中 {failed} 個のコンポーネントの復元に失敗しました。',
  'notify.statusError': 'エラー',
  'notify.statusWarning': '警告',

  'scan.partialWarning': {
    one: '1つのフォルダをスキャンできませんでした。',
    other: '{count}個のフォルダをスキャンできませんでした。',
  },

  'coverSync.failed': 'カバー画像の同期に失敗しました。',
  'coverSync.refreshFailed': 'カバー画像の同期に失敗しました。',

  'nvidia.adminRequired': '管理者権限が必要です',
  'nvidia.relaunchTo': '{action} には管理者として再起動してください。',
  'nvidia.action.changeSetting': '設定を適用する',
  'nvidia.action.revertSetting': '設定を元に戻す',
  'nvidia.changeSettingFailed': '設定の適用に失敗しました',
  'nvidia.revertDefaultFailed': 'デフォルト設定への復元に失敗しました',
  'nvidia.revertBaselineFailed': '初期設定への復元に失敗しました',

  'indicator.relaunchToToggle':
    'DLSS インジケーターを切り替えるには、管理者として再起動してください。',
  'indicator.changeFailed': 'DLSS インジケーターの切り替えに失敗しました',

  'libraries.column.version': 'バージョン',
  'libraries.column.hash': 'ハッシュ',
  'libraries.column.signed': '署名あり',
  'libraries.column.size': 'サイズ',
  'libraries.column.documents': '文書',
  'libraries.column.actions': 'アクション',
  'libraries.documents.openForVersion': '{name} {version} の法的文書を開く',
  'libraries.documents.title': '法的文書',
  'libraries.documents.description': '{name} {version} に適用されます。',
  'libraries.documents.formatPdf': 'PDF',
  'libraries.documents.formatText': 'テキスト',
  'libraries.documents.open': '開く',
  'libraries.documents.openFailed': '文書を開けませんでした',
  'libraries.unsigned': '署名なし',
  'libraries.invalidDate': '無効な日付',
  'libraries.empty.loading': '読み込み中…',
  'libraries.empty.unavailable': 'ライブラリを読み込めませんでした',
  'libraries.empty.none': 'ライブラリが見つかりません',
  'libraries.error.loadFailed': 'ライブラリの読み込みに失敗しました',
  'libraries.error.refreshFailed': 'マニフェストの更新に失敗しました',
  'libraries.error.downloadFailed': 'ダウンロードに失敗しました',
  'libraries.error.deleteFailed': '削除に失敗しました',
  'libraries.error.downloadedRefreshFailed':
    'ライブラリをダウンロードしましたが、状態の更新に失敗しました',
  'libraries.error.deletedRefreshFailed': 'ライブラリを削除しましたが、状態の更新に失敗しました',

  'settings.catalog.source.steam.aria': 'Steam からカバー画像をダウンロード',
  'settings.catalog.source.steam.title': 'Steam',
  'settings.catalog.source.steam.description':
    'パブリック Steam カタログからカバー画像をダウンロードします。',
  'settings.catalog.source.gog.aria': 'GOG からカバー画像をダウンロード',
  'settings.catalog.source.gog.title': 'GOG',
  'settings.catalog.source.gog.description':
    '公式 GOG カタログからカバー画像をダウンロードします。',
  'settings.catalog.source.steamgriddb.aria': 'SteamGridDB からカバー画像をダウンロード',
  'settings.catalog.source.steamgriddb.title': 'SteamGridDB',
  'settings.catalog.source.steamgriddb.description':
    'SteamGridDB からコミュニティのカバー画像をダウンロードします。API キーが必要です。',
  'settings.catalog.artworkReadError': 'カバー画像の設定の読み込みに失敗しました。',
  'settings.catalog.artworkSaveError': 'カバー画像の設定の保存に失敗しました。',

  'user_message.invalid_argument': '無効な入力が提供されました。',
  'user_message.invalid_install_root':
    '1 つのゲームのインストールフォルダーを選択してください。ドライブ、ネットワーク共有のルート、およびシステムフォルダーは追加できません。',
  'user_message.multiple_installs_detected':
    'このフォルダーには複数のゲームがインストールされています。1 つのゲームのインストールフォルダーを選択してください。',
  'user_message.stale_install_inspection':
    '確認中にインストール内容が変更されました。追加する前に更新された結果を確認してください。',
  'user_message.root_correction_cleanup_required':
    'ゲームルートを変更する前に、有効なコンポーネント変更を元に戻す必要があります。',
  'user_message.root_correction_blocked':
    'ゲームルートを変更する前に、既存のカードで有効な状態を解決してください。',
  'user_message.managed_cleanup_ambiguous':
    '安全な復元順序を判断できない重複した管理変更が見つかりました。変更は行われず、リカバリーパッケージが作成されました。',
  'user_message.game_removal_cleanup_failed':
    '元のゲームファイルを復元できなかったため、カードは削除されませんでした。ゲームファイルを確認して、もう一度お試しください。',
  'user_message.invalid_game_reference': 'ゲームが見つかりません。',
  'user_message.invalid_component_reference': 'コンポーネントが見つかりません。',
  'user_message.invalid_artifact_reference': 'アイテムが見つかりません。',
  'user_message.invalid_operation_reference': 'アクションが見つかりません。',
  'user_message.response_serialization_failed': 'リクエストの処理に失敗しました。',
  'user_message.plan_changed_rebuild': 'タスクが古くなっています。再試行してください。',
  'user_message.game_not_in_catalog': 'ゲームはサポートされていません。',
  'user_message.operation_not_found': 'アクションが見つかりません。',
  'user_message.artifact_not_found': 'アイテムが見つかりません。',
  'user_message.component_not_found': 'コンポーネントが見つかりません。',
  'user_message.invalid_operation_state': 'このアクションは現在利用できません。',
  'user_message.operation_could_not_complete': 'アクションを完了できませんでした。',
  'user_message.command_task_failed': 'コマンドの実行に失敗しました。',
  'user_message.storage_failed': 'アプリのカタログの読み書きに失敗しました。',
  'user_message.provider_failed': 'データソースを読み取れませんでした。',
  'user_message.detection_failed': 'ゲームファイルを解析できませんでした。',
  'user_message.steamgriddb_api_key_missing': '設定で SteamGridDB API キーを提供してください。',
  'user_message.unsupported_cover_image_type': 'サポートされていない画像形式です。',
  'user_message.cover_download_failed': 'カバー画像のダウンロードに失敗しました。',
  'user_message.cover_artwork_not_found': 'このゲームのカバー画像が見つかりません。',
  'user_message.cover_file_system_error': 'カバー画像をディスクに保存できませんでした。',
  'user_message.stale_replacement_source':
    'ソースファイルが RenderPilot の外で置き換えまたは変更されたため、この更新を適用できませんでした。バージョンを再度選択してください。ダウンロードが必要になる場合があります。',
  'user_message.nvapi_requires_administrator': 'この設定を変更するには管理者権限が必要です。',

  'suggested_action.refresh_games': 'ゲームリストを更新して再試行してください。',
  'suggested_action.reload_game_details': 'ゲームの詳細を更新して再試行してください。',
  'suggested_action.refresh_candidates': 'リストを更新して再試行してください。',
  'suggested_action.rebuild_plan_or_reload_operations': '表示を更新して再試行してください。',
  'suggested_action.retry_after_required_data': 'しばらく待ってから再試行してください。',
  'suggested_action.inspect_logs': '問題が解決しない場合は、アプリを再起動してみてください。',
  'suggested_action.retry_or_restart': '問題が解決しない場合は、アプリを再起動してみてください。',
  'suggested_action.rebuild_operation_plan': 'アクションをやり直してください。',
  'suggested_action.refresh_or_scan_game_folder':
    'リストを更新するか、フォルダを再スキャンしてください。',
  'suggested_action.relaunch_as_administrator':
    'アプリを管理者として再起動して再試行してください。',

  'settings.about.title': 'アップデート',
  'settings.about.description': 'アプリケーションのアップデートを確認します。',
  'settings.about.version.title': 'アプリのバージョン',
  'settings.about.version.loading': '読み込み中...',
  'settings.about.checkForUpdates': 'アップデートを確認',
  'settings.about.updateInProgress': '更新中…',
  'settings.about.updateAvailable': 'アップデートがあります',
  'settings.about.upToDate': '最新バージョンです',
  'settings.about.updateCheckError': 'アップデートの確認に失敗しました',

  'settings.about.updateDialog.title': 'アップデートがあります',
  'settings.about.updateDialog.versionLine': '{currentVersion} → {version}',
  'settings.about.updateDialog.releaseDate': 'リリース日: {date}',
  'settings.about.updateDialog.releaseNotes': 'リリースノート',
  'settings.about.updateDialog.noNotes': 'このアップデートのリリースノートはありません。',
  'settings.about.updateDialog.notesTruncated': 'リリースノートは短縮されています。',

  'settings.about.updateDialog.installAndRestart': 'インストールして再起動',
  'settings.about.updateDialog.later': '後で',
  'settings.about.updateDialog.close': '閉じる',
  'settings.about.updateDialog.retryDownload': 'ダウンロードを再試行',
  'settings.about.updateDialog.retryInstall': 'インストールを再試行',
  'settings.about.updateDialog.restartNow': '今すぐ再起動',

  'settings.about.updateDialog.downloading': 'アップデートをダウンロード中…',
  'settings.about.updateDialog.downloadingBytes': '{received} ダウンロード済み',
  'settings.about.updateDialog.downloadingBytesTotal': '{received} / {total}',
  'settings.about.updateDialog.verifying': 'アップデートを検証中…',
  'settings.about.updateDialog.verifyingDescription':
    'ダウンロードしたパッケージを確認しています。',
  'settings.about.updateDialog.installing':
    'アップデートをインストール中… アプリは終了し、インストーラーが短く表示される場合があります。',
  'settings.about.updateDialog.restarting': 'アプリを再起動中…',

  'settings.about.updateDialog.prepareErrorTitle': 'ダウンロードまたは検証に失敗しました',
  'settings.about.updateDialog.prepareErrorDescription':
    'アップデートをダウンロードまたは検証できませんでした。接続を確認して再試行してください。',
  'settings.about.updateDialog.installErrorTitle': 'インストールに失敗しました',
  'settings.about.updateDialog.installErrorDescription':
    'アップデートをインストールできませんでした。再試行できます。',
  'settings.about.updateDialog.restartRequiredTitle': '再起動が必要です',
  'settings.about.updateDialog.restartRequiredDescription':
    'アップデートはインストールされましたが、アプリを自動で再起動できませんでした。RenderPilot を手動で再起動して更新を完了してください。',

  'settings.about.updateDialog.progressAria': 'ダウンロード進捗: {percent} パーセント',
  'settings.about.updateDialog.indeterminateProgressAria': 'ダウンロード進捗',
  // ── Game details: RenoDX ──
  'gameDetails.renodx.title': 'RenoDX HDR',
  'gameDetails.renodx.description':
    'RenoDX の ReShade アドオンで、このゲームに HDR とトーンマッピングを追加します。',
  'gameDetails.renodx.loading': '利用可否を確認中…',
  'gameDetails.renodx.installError': 'RenoDX のインストールに失敗しました',
  'gameDetails.renodx.uninstallError': 'RenoDX の削除に失敗しました',
  'gameDetails.renodx.switchError': 'ReShade チャンネルの切り替えに失敗しました',
  'gameDetails.renodx.unsupported': 'このゲームに利用できる RenoDX プロファイルはありません。',
  'gameDetails.renodx.incompatible': 'RenoDX をインストールできません：{reason}。',
  'gameDetails.renodx.status.label': 'Status',
  'gameDetails.renodx.statusInstalled': 'インストール済み',
  'gameDetails.renodx.actionInstall': 'インストール',
  'gameDetails.renodx.actionUninstall': 'RenoDX を削除',
  'gameDetails.renodx.actionRepair': '修復',
  'gameDetails.renodx.uninstallConfirmTitle': 'このゲームから RenoDX を削除しますか？',
  'gameDetails.renodx.uninstallConfirmBody':
    'RenoDX アドオンを削除し、RenoDX セットアップ中に変更された ReShade ファイルだけを復元します。',
  'gameDetails.renodx.uninstallConfirmAction': '削除',
  'gameDetails.renodx.installing': 'インストール中…',
  'gameDetails.renodx.confirmTitle':
    'アンチチートのリスクがあっても RenoDX をインストールしますか？',
  'gameDetails.renodx.cancel': 'キャンセル',
  // ── Game details: RenoDX shared Vulkan layer ──
  'gameDetails.renodx.vulkanLayer.removeError':
    '共有 ReShade Vulkan レイヤーを削除できませんでした。',
  'gameDetails.renodx.vulkanLayer.title': '共有 Vulkan レイヤー',
  'gameDetails.renodx.vulkanLayer.removeConfirmTitle': '共有 Vulkan レイヤーを削除しますか？',
  'gameDetails.renodx.vulkanLayer.removeConfirmBody':
    '共有 ReShade Vulkan レイヤーを削除すると、すべての Vulkan RenoDX ゲームに影響します。続行しますか？',
  'gameDetails.renodx.vulkanLayer.openSettings': 'RenoDX 設定を開く',
  'gameDetails.renodx.vulkanLayer.externalReadOnly':
    '既存の Vulkan レイヤーを検出しました。このバージョンでは読み取り専用です',
  'gameDetails.renodx.vulkanLayer.state.not_installed': '未インストール',
  'gameDetails.renodx.vulkanLayer.state.installed': 'インストール済み',
  'gameDetails.renodx.vulkanLayer.state.installed_disabled': 'Disabled in registry',
  'gameDetails.renodx.vulkanLayer.state.external_read_only': '読み取り専用',
  'gameDetails.renodx.vulkanLayer.state.conflict': '競合',
  'gameDetails.renodx.vulkanLayer.state.needs_repair': '修復が必要',
  'gameDetails.renodx.vulkanLayer.state.unsupported': '非対応',
  'gameDetails.renodx.vulkanLayer.action.install': 'インストール',
  'gameDetails.renodx.vulkanLayer.action.update': '更新',
  'gameDetails.renodx.vulkanLayer.action.switch_channel': 'チャンネルを切り替え',
  'gameDetails.renodx.vulkanLayer.action.repair': 'レイヤーを修復',
  'gameDetails.renodx.vulkanLayer.action.remove': '削除',
  'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected':
    '既存の Vulkan レイヤーが検出されました。',
  'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest':
    '複数の ReShade レイヤーマニフェストが登録されています。',
  'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility':
    'ローダーの可視性が不明確です。',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll': 'レイヤー DLL がありません。',
  'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll':
    'The layer DLL could not be read (permission denied or locked).',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest': 'The layer manifest is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing':
    'レイヤーファイルはありますが、Vulkan ローダー登録がありません。',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled':
    'The loader registry entry is disabled.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture':
    'レイヤーのアーキテクチャが非対応です。',
  'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated':
    'レイヤーは HKCU に登録されており、昇格して実行されたゲームでは読み込まれない可能性があります。',
  'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed':
    'レイヤーマニフェストを解析できませんでした。',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable':
    '必要なレジストリスコープに書き込めません。',
  'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied':
    'オペレーティングシステムが要求された操作を拒否しました。',
  'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed':
    'バックエンドの検証に失敗しました。レイヤー要確認です。',
  'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch':
    'The layer DLL hash does not match the expected version.',
  'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback':
    'The layer DLL is missing; using advisory database record.',
  // ── Game details: RenoDX incompatibility reasons ──
  'gameDetails.renodx.reason.api_unsupported': '非対応のグラフィックス API',
  'gameDetails.renodx.reason.api_not_allowed': 'このゲームでは許可されていないグラフィックス API',
  'gameDetails.renodx.reason.arch_unknown': '実行ファイルのアーキテクチャが不明',
  'gameDetails.otherTab': 'その他',
  'gameDetails.renodx.unavailable': '現在 RenoDX は利用できません。',
  'renodx.generic.universal': 'ユニバーサル RenoDX',
  'renodx.generic.unity': 'ユニバーサル RenoDX (Unity)',
  'gameDetails.renodx.generic.profileTooltip': '共有エンジンプロファイルを使用しています。',
  'renodx.phase.finalizing': '完了処理中…',
  'luma.phase.finalizing': '完了処理中…',
  'gameDetails.renodx.confidenceLabel': 'RenoDX 互換性',
  'gameDetails.renodx.confidenceVerified': '動作します',
  'gameDetails.renodx.confidenceExperimental': '対応中',
  'gameDetails.renodx.confidenceUntested': '未確認',
  'gameDetails.renodx.external':
    'この RenoDX アドオンは外部で配布されており、手動でダウンロードする必要があります。',
  'gameDetails.renodx.actionOpenExternal': 'ダウンロードページを開く',
  'gameDetails.renodx.external.installFromFile': 'ファイルからインストール',
  'gameDetails.renodx.external.dropHint':
    'アドオンをダウンロードして、ここにドロップするかファイルを選択してください。',
  'gameDetails.renodx.external.invalidFile':
    'このファイルは RenoDX アドオン (.addon64 / .addon32) ではありません。',
  'gameDetails.renodx.fileInstall.title': '手動インストール',
  'gameDetails.renodx.fileInstall.chooseFile': 'アドオンファイルを選択…',
  'gameDetails.renodx.fileInstall.chooseAnother': '別のファイルを選択',
  'gameDetails.renodx.fileInstall.expected': '想定アドオン: {name}',
  'gameDetails.renodx.fileInstall.confirm': '{fileName} をインストールしますか？',
  'gameDetails.renodx.fileInstall.errorExtension':
    'このファイルは RenoDX アドオンではありません（.addon64 / .addon32）。',
  'gameDetails.renodx.fileInstall.errorArch':
    'このアドオンは {addon} ですが、ゲームは {game} です。一致するアドオンをダウンロードしてください。',
  'gameDetails.renodx.fileInstall.warnName':
    '想定されるアドオン（{expected}）と異なるようです。確信がある場合のみインストールしてください。',
  'gameDetails.renodx.nativeHdr':
    'このゲームはすでにネイティブ HDR に対応しているため、RenoDX は不要です。',
  'gameDetails.renodx.blacklisted': 'このゲームには RenoDX は推奨されません。',
  'gameDetails.renodx.updatesNotTracked': '更新は追跡されません',
  'gameDetails.renodx.channel.label': 'ReShade ホストチャンネル',
  'gameDetails.renodx.channel.hostLabel': 'ReShade ホスト',
  'gameDetails.renodx.channel.stable': 'Stable',
  'gameDetails.renodx.channel.nightly': 'Nightly',
  'gameDetails.renodx.host.version': '{version}',
  'gameDetails.renodx.host.versionUnknown': 'ReShade のバージョン不明',
  'gameDetails.renodx.host.addons.none': 'アドオン非対応',
  'gameDetails.renodx.host.addons.unknown': 'アドオン対応は不明',
  'gameDetails.renodx.host.action.update_host': '更新あり',
  'gameDetails.renodx.host.action.repair_host': 'RenoDX アドオン対応のため ReShade を修復',
  'gameDetails.renodx.host.customBuild':
    'カスタムビルド（GShadeなど）— 更新はご自身で管理してください',
  'gameDetails.renodx.host.conflictMultiple':
    '複数の ReShade ホストを検出 — アクティブなスロットを確認してください',
  'gameDetails.renodx.host.conflictBlocksInstall':
    'このゲームが使う ReShade スロットが別のファイルに占有されているか、ReShade が別のスロットにあります。インストール前に解決してください。',
  'gameDetails.renodx.actionUpdate': '更新',
  'gameDetails.renodx.updating': '更新中…',
  'gameDetails.renodx.updateError': 'RenoDX の更新に失敗しました',
  'gameDetails.renodx.actionInstallDlssFix': 'インストール',
  'gameDetails.renodx.actionRemoveDlssFix': '削除',
  'gameDetails.renodx.dlssFixInstallError': 'DLSS-Fixのインストールに失敗しました',
  'gameDetails.renodx.dlssFixRemoveError': 'DLSS-Fixの削除に失敗しました',
  'gameDetails.renodx.fresh.label': 'アップデート',
  'gameDetails.renodx.fresh.current': '最新',
  'gameDetails.renodx.fresh.available': 'アップデートあり',
  'gameDetails.renodx.fresh.channelMismatch': 'チャンネル変更が利用可能',
  'gameDetails.renodx.fresh.validationRequired': '検証が必要です',
  'gameDetails.renodx.fresh.unknown': '確認できませんでした',
  'gameDetails.renodx.fresh.checking': '確認中…',
  'gameDetails.renodx.addonDated': 'アドオン日付: {date}',
  'gameDetails.renodx.installedOn': 'インストール: {date}',
  'gameDetails.renodx.lastChecked': '確認: {time}',
  'gameDetails.renodx.lastCheckedNever': '未確認',
  'gameDetails.renodx.actionCheckUpdates': 'アップデートを確認',
  'gameDetails.renodx.component.reshade': 'ReShade ホスト',
  'gameDetails.renodx.component.addon': 'RenoDX アドオン',
  'gameDetails.renodx.component.addonDesc': 'このゲーム用の HDR アドオン',
  'gameDetails.renodx.component.addonDisabled':
    'インストール済みですが ReShade.ini で無効化されています',
  'gameDetails.renodx.component.addonFileInstall':
    'ファイルからインストール — アップデート追跡なし',
  'gameDetails.renodx.component.dlssFix': 'DLSS-Fix',
  'gameDetails.renodx.component.dlssFixDesc': 'DLSS Frame Generation 使用時のちらつきを修正',
  'gameDetails.renodx.component.dlssFixOffer': '利用可能 — DLSS Frame Generation のちらつきを防止',
  'gameDetails.renodx.component.dlssFixHint':
    'ReShade 全般で動作し、RenoDX 専用ではありません。ReShade をフレーム生成フレームではなくゲーム本来のフレームに描画させ、ゲームが Streamline を正しく実装している場合は DLSS（アップスケーリング）を ReShade から隠します。',
  'gameDetails.renodx.attribution': 'RenoDX by clshortfuse。',
  'gameDetails.renodx.attributionLink': 'プロジェクトを見る',
  // ── Game details: shared add-on copy (RenoDX + Luma) ──
  'gameDetails.addon.riskSafe': 'アンチチートは検出されていません — 安全にインストールできます。',
  'gameDetails.addon.riskWarn':
    'アンチチートが検出されました — インストールするとBANされる可能性があります。',
  'addon.risk.sp_safe':
    '既知のアンチチートシグネチャは検出されませんでした — {addonName} のインストールはおそらく安全ですが、保証はされません。',
  'addon.risk.anticheat_detected':
    'アンチチートシグネチャが検出されました — {addonName} をインストールするとBANの恐れがあります。',
  'gameDetails.addon.confirmAccept': 'それでもインストールする',
  'gameDetails.addon.confirmBody':
    'このゲームはアンチチートを使用しています。ReShade アドオンがこれを誘発し、BANされる可能性があります。自己責任で続行してください。',
  'gameDetails.addon.fullAddonWarning':
    'ReShade のフルアドオン対応は、マルチプレイヤーやアンチチート保護されたゲームでは安全でない場合があります。',
  'gameDetails.addon.blockedByOtherAddon.tracked':
    'このゲームには {installedAddon} がインストールされています — {blockedAddon} をインストールする前にアンインストールしてください。',
  'gameDetails.addon.blockedByOtherAddon.unmanaged':
    'このゲームのディスク上に {installedAddon} のファイルが見つかりました — {blockedAddon} をインストールする前に削除してください。',
  'addon.availability.loadFailed': '確認できませんでした',
  'addon.availability.retry': '再試行',
  'addon.availability.checking': '確認中…',
  // ── Game details: Luma ──
  'gameDetails.luma.title': 'Luma Framework',
  'gameDetails.luma.description': 'このゲームで利用可能な Luma の機能を以下に示します。',
  'gameDetails.luma.loading': '利用可否を確認しています…',
  'gameDetails.luma.installError': 'Luma のインストールに失敗しました',
  'gameDetails.luma.uninstallError': 'Luma の削除に失敗しました',
  'gameDetails.luma.updateError': 'Luma の更新に失敗しました',
  'gameDetails.luma.repairError': 'Luma の修復に失敗しました',
  'gameDetails.luma.unsupported': 'このゲームに対応する Luma プロファイルはありません。',
  'gameDetails.luma.incompatible': 'Luma をインストールできません: {reason}。',
  'gameDetails.luma.blacklisted': 'このゲームには Luma を推奨しません。',
  'gameDetails.luma.unavailable': 'Luma は現在利用できません。',
  'gameDetails.luma.unmanagedPresent':
    '記録のない既存の Luma インストールがディスク上に見つかりました。手動で削除してから再インストールしてください。',
  'gameDetails.luma.installTornWarning':
    '前回のインストールが正常に完了しませんでした。再度インストールするとクリーンアップして修復されます。',
  'gameDetails.luma.installTornWarningInstalled':
    '前回の操作が正常に完了しませんでした。修復（または更新が表示されている場合は更新）でインストールを整合させてください。',
  'gameDetails.luma.status.label': 'ステータス',
  'gameDetails.luma.statusInstalled': 'インストール済み',
  'gameDetails.luma.actionInstall': 'インストール',
  'gameDetails.luma.installing': 'インストール中…',
  'gameDetails.luma.actionUninstall': 'Luma を削除',
  'gameDetails.luma.actionRepair': '修復',
  'gameDetails.luma.actionUpdate': '更新',
  'gameDetails.luma.updating': '更新中…',
  'gameDetails.luma.actionCheckUpdates': '更新を確認',
  'gameDetails.luma.uninstallConfirmTitle': 'このゲームから Luma を削除しますか？',
  'gameDetails.luma.uninstallConfirmBody':
    'Luma を削除します。Luma が DLSS DLL を管理している場合、その Library Swap を元に戻し、Luma 導入前の状態を正確に復元します。再利用された DLL と独立した Swap は変更しません。',
  'gameDetails.luma.uninstallConfirmAction': '削除',
  'gameDetails.luma.confirmTitle': 'アンチチートのリスクを承知の上で Luma をインストールしますか？',
  'gameDetails.luma.vcredistWarning':
    'このシステムに最新の Visual C++ Redistributable がない可能性があります。Luma が読み込まれない場合は Redistributable をインストールしてください。',
  'gameDetails.luma.vcredistLink': 'Redistributable をダウンロード',
  'gameDetails.luma.dgvoodoo.managed':
    'RenderPilot がこの Luma プロファイル用に dgVoodoo2 {version} をインストールして設定します。',
  // ── Game details: Luma confidence ──
  'gameDetails.luma.confidenceLabel': 'Luma 互換性',
  'gameDetails.luma.confidenceVerified': '動作確認済み',
  'gameDetails.luma.confidenceExperimental': '対応中',
  'gameDetails.luma.confidenceUntested': '未検証',
  'gameDetails.luma.generic.engineUnreal': 'Unreal Engine',
  'gameDetails.luma.generic.engineUnity': 'Unity',
  'gameDetails.luma.generic.profileTooltip': '共有エンジンプロファイルを使用しています。',
  'gameDetails.luma.features.title': '機能',
  'gameDetails.luma.features.dlssFsr': 'DLSS / FSR',
  'gameDetails.luma.features.hdr': 'HDR',
  'gameDetails.luma.features.supported': '対応',
  'gameDetails.luma.features.unsupported': '非対応',
  'gameDetails.luma.features.experimental': '実験的',
  'gameDetails.luma.features.unknown': '不明',
  // ── Game details: Luma incompatibility reasons ──
  'gameDetails.luma.reason.api_unsupported': '対応していないグラフィックスAPI',
  'gameDetails.luma.reason.api_not_allowed': 'このゲームでは許可されていないグラフィックスAPI',
  'gameDetails.luma.reason.arch_unknown': '実行ファイルのアーキテクチャが不明',
  'gameDetails.luma.reason.arch_mismatch':
    '実行ファイルのアーキテクチャがこのアドオンと一致しません',
  // ── Game details: Luma ReShade host ──
  'gameDetails.luma.channel.stable': 'Stable',
  'gameDetails.luma.channel.nightly': 'Nightly',
  'gameDetails.luma.host.version': '{version}',
  'gameDetails.luma.host.versionUnknown': 'バージョン不明',
  'gameDetails.luma.host.addons.none': 'アドオン非対応',
  'gameDetails.luma.host.addons.unknown': 'アドオン対応状況不明',
  'gameDetails.luma.host.action.update_host': '更新あり',
  'gameDetails.luma.host.action.repair_host': 'Luma アドオン対応のため ReShade を修復',
  'gameDetails.luma.host.customBuild': 'カスタムビルド（GShade など） — 更新は自己管理してください',
  'gameDetails.luma.host.conflictMultiple':
    '複数の ReShade ホストが見つかりました — アクティブなスロットを確認してください',
  'gameDetails.luma.host.conflictBlocksInstall':
    'このゲームが使用する ReShade スロットは既存のファイルに占有されているか、ReShade が別のスロットにあります — インストール前に解決してください。',
  // ── Game details: Luma freshness / timestamps ──
  'gameDetails.luma.fresh.label': 'バージョン',
  'gameDetails.luma.fresh.current': '最新',
  'gameDetails.luma.fresh.available': '更新あり',
  'gameDetails.luma.fresh.channelMismatch': 'チャンネル変更が可能です',
  'gameDetails.luma.fresh.validationRequired': '検証が必要です',
  'gameDetails.luma.fresh.unknown': '確認できませんでした',
  'gameDetails.luma.fresh.checking': '確認中…',
  'gameDetails.luma.updatesNotTracked': '更新は追跡されていません',
  'gameDetails.luma.addonDated': 'アドオンの日付: {date}',
  'gameDetails.luma.installedOn': '{date} にインストール',
  'gameDetails.luma.lastChecked': '{time}に確認',
  'gameDetails.luma.lastCheckedNever': 'まだ確認されていません',
  // ── Game details: Luma components ──
  'gameDetails.luma.component.reshade': 'ReShade ホスト',
  'gameDetails.luma.component.addon': 'Luma アドオン',
  'gameDetails.luma.component.addonDesc': 'このゲーム用の Luma 機能',
  'gameDetails.luma.component.dgvoodoo': 'dgVoodoo2 ラッパー',
  'gameDetails.luma.component.dgvoodooDesc': '管理対象の D3D9 ブリッジ、バージョン {version}',
  // ── Game details: Luma launch arguments ──
  'gameDetails.luma.launchArgs.instructions.steam':
    'Steam 経由でゲームを起動する場合は、ゲームを右クリック → プロパティ → 全般 → 起動オプションで追加してください。',
  'gameDetails.luma.launchArgs.instructions.gog':
    'GOG Galaxy 経由でゲームを起動する場合は、ゲーム設定 → インストールの管理 → 構成で追加してください。',
  'gameDetails.luma.launchArgs.instructions.epic':
    'Epic Games Launcher 経由でゲームを起動する場合は、ゲームを右クリック → 管理 → 追加のコマンドライン引数で追加してください。',
  'gameDetails.luma.launchArgs.instructions.ea':
    'EA app 経由でゲームを起動する場合は、ゲームを選択 → 管理 → プロパティを表示 → 詳細な起動オプションで追加してください。',
  'gameDetails.luma.launchArgs.instructions.ubisoft':
    'Ubisoft Connect 経由でゲームを起動する場合は、ゲームを選択 → プロパティ → 起動引数を追加で追加してください。',
  'gameDetails.luma.launchArgs.instructions.other':
    '実際にゲームを起動する方法に引数を追加してください。ランチャー、ショートカットのリンク先、バッチファイル、または別のローダーに設定します。',
  'gameDetails.luma.launchArgs.title': '起動引数が必要です',
  'gameDetails.luma.launchArgs.dx11Title': 'この Luma プロファイルには DirectX 11 が必要です',
  'gameDetails.luma.launchArgs.copyStep': '必要な起動引数をコピーしてください:',
  'gameDetails.luma.launchArgs.copy': '引数をコピー',
  'gameDetails.luma.launchArgs.copied': 'コピーしました',
  'gameDetails.luma.launchArgs.copyFailed': '起動引数をコピーできませんでした',
  // ── Game details: Luma attribution ──
  'gameDetails.luma.attribution': 'Luma Framework by Filoppi。',
  'gameDetails.luma.attributionLink': 'プロジェクトを見る',
  'gameDetails.luma.guidance.gameSetting': 'ゲーム内設定',
  'gameDetails.luma.guidance.engineIni': '手動 INI 変更',
  'gameDetails.luma.guidance.launchArgument': '起動引数',
  'gameDetails.luma.guidance.warning': '重要',
  'gameDetails.luma.guidance.compatibility': '互換性に関する注意',
  'gameDetails.luma.guidance.externalTool': 'サードパーティーツール',
  'gameDetails.luma.guidance.copy': 'コピー',
  'gameDetails.luma.guidance.copied': 'コピーしました',
  'gameDetails.luma.guidance.copyFailed': 'コピーできませんでした',
};
