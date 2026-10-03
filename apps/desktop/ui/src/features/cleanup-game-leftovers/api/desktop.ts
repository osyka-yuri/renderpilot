import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type {
  CleanupOutcome,
  LeaveOutcome,
  ListRetiredGameLeftoversOutput,
  RetiredGameLeftoversRequest,
} from '../model/types';

export function listRetiredGameLeftovers(): Promise<ListRetiredGameLeftoversOutput> {
  return invokeDesktop('list_retired_game_leftovers');
}

export function cleanRetiredGameLeftovers(
  request: RetiredGameLeftoversRequest,
): Promise<CleanupOutcome> {
  return invokeDesktop('clean_retired_game_leftovers', actionPayload(request));
}

export function leaveRetiredGameLeftovers(
  request: RetiredGameLeftoversRequest,
): Promise<LeaveOutcome> {
  return invokeDesktop('leave_retired_game_leftovers', actionPayload(request));
}

function actionPayload(request: RetiredGameLeftoversRequest): RetiredGameLeftoversRequest {
  return {
    gameId: requireNonBlankString(request.gameId, 'gameId'),
    intent: requireNonBlankString(request.intent, 'intent'),
  };
}
