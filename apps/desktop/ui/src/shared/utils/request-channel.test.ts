import { describe, expect, it } from 'vitest';
import { createDisposableRequestChannel, createRequestChannel } from './request-channel';

describe('createRequestChannel', () => {
  it('starts from the provided initial request id', () => {
    const channel = createRequestChannel(10);

    const requestId = channel.begin();

    expect(requestId).toBe(11);
    expect(channel.isActive(11)).toBe(true);
    expect(channel.isActive(10)).toBe(false);
  });

  it('marks previous request ids as stale after begin', () => {
    const channel = createRequestChannel();

    const firstRequestId = channel.begin();
    const secondRequestId = channel.begin();

    expect(channel.isActive(firstRequestId)).toBe(false);
    expect(channel.isActive(secondRequestId)).toBe(true);
  });

  it('invalidates the current active request', () => {
    const channel = createRequestChannel();

    const requestId = channel.begin();

    expect(channel.isActive(requestId)).toBe(true);

    channel.invalidate();

    expect(channel.isActive(requestId)).toBe(false);
  });

  it('invalidates all previous requests after several operations', () => {
    const channel = createRequestChannel();

    const firstRequestId = channel.begin();
    channel.invalidate();

    const secondRequestId = channel.begin();
    channel.invalidate();

    expect(channel.isActive(firstRequestId)).toBe(false);
    expect(channel.isActive(secondRequestId)).toBe(false);
  });

  it('normalizes non-integer initial request id', () => {
    const channel = createRequestChannel(1.9);

    expect(channel.begin()).toBe(2);
  });

  it('throws for non-finite initial request id', () => {
    expect(() => createRequestChannel(Number.NaN)).toThrow(TypeError);
    expect(() => createRequestChannel(Number.POSITIVE_INFINITY)).toThrow(TypeError);
    expect(() => createRequestChannel(Number.NEGATIVE_INFINITY)).toThrow(TypeError);
  });
});

describe('createDisposableRequestChannel', () => {
  it('exposes disposal state while preserving request semantics', () => {
    let disposed = false;
    const channel = createDisposableRequestChannel(() => disposed, 3);

    const requestId = channel.begin();

    expect(requestId).toBe(4);
    expect(channel.isActive(requestId)).toBe(true);
    expect(channel.isDisposed()).toBe(false);

    disposed = true;

    expect(channel.isDisposed()).toBe(true);
  });
});
