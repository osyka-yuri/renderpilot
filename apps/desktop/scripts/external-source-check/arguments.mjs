import path from 'node:path';

const USAGE = 'Usage: node scripts/check-external-source-contracts.mjs --producer-root <path>';

export function parseExternalSourceCheckArguments(args) {
  if (
    args.length !== 2 ||
    args[0] !== '--producer-root' ||
    typeof args[1] !== 'string' ||
    args[1].trim() === ''
  ) {
    throw new Error(USAGE);
  }
  return { producerRoot: path.resolve(args[1]) };
}
