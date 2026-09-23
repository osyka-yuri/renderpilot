import {
  isLocalErrorCode as isLocalErrorCodeFromContract,
  LOCAL_ERROR_CONTRACT as localErrorContract,
  type LocalErrorCode as ContractLocalErrorCode,
  type LocalErrorSpec as ContractLocalErrorSpec,
} from '@shared/error-contract';

export const isLocalErrorCode = isLocalErrorCodeFromContract;
export const LOCAL_ERROR_CONTRACT = localErrorContract;
export type LocalErrorCode = ContractLocalErrorCode;
export type LocalErrorSpec = ContractLocalErrorSpec;
