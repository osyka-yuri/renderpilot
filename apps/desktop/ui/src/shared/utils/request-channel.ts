export type RequestId = number;

export type RequestChannel = {
  begin: () => RequestId;
  isActive: (requestId: RequestId) => boolean;
  invalidate: () => void;
};

export type DisposableRequestChannel = RequestChannel & {
  isDisposed: () => boolean;
};

function normalizeInitialRequestId(value: number): RequestId {
  if (!Number.isFinite(value)) {
    throw new TypeError('initialRequestId must be a finite number.');
  }

  return Math.trunc(value);
}

export function createRequestChannel(initialRequestId = 0): RequestChannel {
  let currentRequestId = normalizeInitialRequestId(initialRequestId);

  function nextRequestId(): RequestId {
    currentRequestId += 1;
    return currentRequestId;
  }

  return {
    begin: nextRequestId,

    isActive: (requestId: RequestId): boolean => requestId === currentRequestId,

    invalidate: (): void => {
      nextRequestId();
    },
  };
}

export function createDisposableRequestChannel(
  isDisposed: () => boolean,
  initialRequestId = 0,
): DisposableRequestChannel {
  const channel = createRequestChannel(initialRequestId);

  return {
    ...channel,
    isDisposed,
  };
}
