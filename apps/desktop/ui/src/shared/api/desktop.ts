import { invoke } from '@tauri-apps/api/core';

import { normalizeCommandError } from './errors';
import {
  isDesktopPreviewMode,
  mockApplyOperationPlan,
  mockBuildSwapPlan,
  mockGetGameCards,
  mockGetGameDetails,
  mockRollbackOperation,
  mockScanAutoLibraries,
  mockScanManualFolder,
} from './mock-desktop';
import type {
  ApplyOperationResult,
  AutoScanResponse,
  GameCard,
  GameDetails,
  RollbackOperationResult,
  SwapPlan,
} from './types';

export type ScanManualFolderResult = {
  games: GameDetails[];
};

type DesktopCommandContract = {
  scan_manual_folder: {
    payload: { path: string };
    result: ScanManualFolderResult;
  };
  scan_auto_libraries: {
    payload: undefined;
    result: AutoScanResponse;
  };
  get_game_cards: {
    payload: undefined;
    result: GameCard[];
  };
  get_game_details: {
    payload: { gameId: string };
    result: GameDetails;
  };
  build_swap_plan: {
    payload: { gameId: string; componentId: string; artifactId: string };
    result: SwapPlan;
  };
  apply_operation_plan: {
    payload: { operationId: string; confirmationToken: string };
    result: ApplyOperationResult;
  };
  rollback_operation: {
    payload: { operationId: string };
    result: RollbackOperationResult;
  };
};

type DesktopCommand = keyof DesktopCommandContract;

type CommandPayload<Command extends DesktopCommand> = DesktopCommandContract[Command]['payload'];

type CommandResult<Command extends DesktopCommand> = DesktopCommandContract[Command]['result'];

type CommandArgs<Command extends DesktopCommand> =
  CommandPayload<Command> extends undefined ? [] : [payload: CommandPayload<Command>];

type PreviewHandlers = {
  [Command in DesktopCommand]: (
    payload: CommandPayload<Command>,
  ) => Promise<CommandResult<Command>>;
};

const previewHandlers: PreviewHandlers = {
  scan_manual_folder: ({ path }) => mockScanManualFolder(path),

  scan_auto_libraries: () => mockScanAutoLibraries(),

  get_game_cards: () => mockGetGameCards(),

  get_game_details: ({ gameId }) => mockGetGameDetails(gameId),

  build_swap_plan: ({ gameId, componentId, artifactId }) =>
    mockBuildSwapPlan(gameId, componentId, artifactId),

  apply_operation_plan: ({ operationId, confirmationToken }) =>
    mockApplyOperationPlan(operationId, confirmationToken),

  rollback_operation: ({ operationId }) => mockRollbackOperation(operationId),
};

async function invokeDesktopPreview<Command extends DesktopCommand>(
  command: Command,
  payload: CommandPayload<Command>,
): Promise<CommandResult<Command>> {
  return previewHandlers[command](payload);
}

async function invokeDesktop<Command extends DesktopCommand>(
  command: Command,
  ...args: CommandArgs<Command>
): Promise<CommandResult<Command>> {
  const payload = args[0];

  try {
    if (isDesktopPreviewMode()) {
      return await invokeDesktopPreview(command, payload);
    }

    return await invoke<CommandResult<Command>>(command, payload);
  } catch (error) {
    throw normalizeCommandError(error);
  }
}

export async function scanManualFolder(path: string): Promise<ScanManualFolderResult> {
  return invokeDesktop('scan_manual_folder', { path });
}

export async function scanAutoLibraries(): Promise<AutoScanResponse> {
  return invokeDesktop('scan_auto_libraries');
}

export async function getGameCards(): Promise<GameCard[]> {
  return invokeDesktop('get_game_cards');
}

export async function getGameDetails(gameId: string): Promise<GameDetails> {
  return invokeDesktop('get_game_details', { gameId });
}

export async function buildSwapPlan(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
  return invokeDesktop('build_swap_plan', {
    gameId,
    componentId,
    artifactId,
  });
}

export async function applyOperationPlan(
  operationId: string,
  confirmationToken: string,
): Promise<ApplyOperationResult> {
  return invokeDesktop('apply_operation_plan', {
    operationId,
    confirmationToken,
  });
}

export async function rollbackOperation(operationId: string): Promise<RollbackOperationResult> {
  return invokeDesktop('rollback_operation', { operationId });
}
