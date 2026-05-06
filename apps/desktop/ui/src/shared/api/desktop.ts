import { invoke } from '@tauri-apps/api/core';

import { normalizeCommandError } from './errors';
import {
  isDesktopPreviewMode,
  mockApplyOperationPlan,
  mockBuildSwapPlan,
  mockGetGameCards,
  mockGetGameDetails,
  mockGetSystemAppearance,
  mockRollbackOperation,
  mockScanManualFolder,
} from './mock-desktop';
import type {
  ApplyOperationResult,
  GameCard,
  GameDetails,
  RollbackOperationResult,
  SwapPlan,
} from './types';

type DesktopCommand =
  | 'get_system_appearance'
  | 'scan_manual_folder'
  | 'get_game_cards'
  | 'get_game_details'
  | 'build_swap_plan'
  | 'apply_operation_plan'
  | 'rollback_operation';

export type SystemAppearance = {
  accentColor: string | null;
};

type DesktopCommandPayloads = {
  get_system_appearance: undefined;
  scan_manual_folder: { path: string };
  get_game_cards: undefined;
  get_game_details: { gameId: string };
  build_swap_plan: { gameId: string; componentId: string; artifactId: string };
  apply_operation_plan: { operationId: string; confirmationToken: string };
  rollback_operation: { operationId: string };
};

type DesktopCommandResults = {
  get_system_appearance: SystemAppearance;
  scan_manual_folder: GameDetails;
  get_game_cards: GameCard[];
  get_game_details: GameDetails;
  build_swap_plan: SwapPlan;
  apply_operation_plan: ApplyOperationResult;
  rollback_operation: RollbackOperationResult;
};

async function invokeDesktop<Command extends DesktopCommand>(
  command: Command,
  payload?: DesktopCommandPayloads[Command],
): Promise<DesktopCommandResults[Command]> {
  if (isDesktopPreviewMode()) {
    return invokeDesktopPreview(command, payload);
  }

  try {
    return await invoke<DesktopCommandResults[Command]>(command, payload);
  } catch (error) {
    throw normalizeCommandError(error);
  }
}

async function invokeDesktopPreview<Command extends DesktopCommand>(
  command: Command,
  payload?: DesktopCommandPayloads[Command],
): Promise<DesktopCommandResults[Command]> {
  switch (command) {
    case 'get_system_appearance':
      return await mockGetSystemAppearance() as DesktopCommandResults[Command];
    case 'scan_manual_folder': {
      const scanPayload = payload as DesktopCommandPayloads['scan_manual_folder'] | undefined;
      return await mockScanManualFolder(scanPayload?.path ?? 'C:/Preview Game') as DesktopCommandResults[Command];
    }
    case 'get_game_cards':
      return await mockGetGameCards() as DesktopCommandResults[Command];
    case 'get_game_details': {
      const detailsPayload = payload as DesktopCommandPayloads['get_game_details'] | undefined;
      return await mockGetGameDetails(detailsPayload?.gameId ?? '') as DesktopCommandResults[Command];
    }
    case 'build_swap_plan': {
      const planPayload = payload as DesktopCommandPayloads['build_swap_plan'] | undefined;
      return await mockBuildSwapPlan(
        planPayload?.gameId ?? '',
        planPayload?.componentId ?? '',
        planPayload?.artifactId ?? '',
      ) as DesktopCommandResults[Command];
    }
    case 'apply_operation_plan': {
      const applyPayload = payload as DesktopCommandPayloads['apply_operation_plan'] | undefined;
      return await mockApplyOperationPlan(
        applyPayload?.operationId ?? '',
        applyPayload?.confirmationToken ?? '',
      ) as DesktopCommandResults[Command];
    }
    case 'rollback_operation': {
      const rollbackPayload = payload as DesktopCommandPayloads['rollback_operation'] | undefined;
      return await mockRollbackOperation(rollbackPayload?.operationId ?? '') as DesktopCommandResults[Command];
    }
  }
}

export async function scanManualFolder(path: string): Promise<GameDetails> {
  return invokeDesktop('scan_manual_folder', { path });
}

export async function getSystemAppearance(): Promise<SystemAppearance> {
  return invokeDesktop('get_system_appearance');
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