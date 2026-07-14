import { describe, expect, it } from 'vitest';

import {
  applyDownloadEvent,
  EMPTY_PROGRESS,
  toCompletedProgressView,
  toProgressView,
} from './progress';

describe('applyDownloadEvent', () => {
  it('starts from the empty state', () => {
    expect(EMPTY_PROGRESS).toEqual({
      totalBytes: null,
      receivedBytes: 0,
      networkFinished: false,
    });
  });

  it('handles Started with a known content length', () => {
    const next = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 1000,
    });

    expect(next).toEqual({
      totalBytes: 1000,
      receivedBytes: 0,
      networkFinished: false,
    });
    expect(toProgressView(next).percent).toBe(0);
  });

  it('handles Started without content length', () => {
    const next = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: null,
    });

    expect(next.totalBytes).toBeNull();
    expect(toProgressView(next).percent).toBeNull();
  });

  it('rejects Started with zero or invalid content length', () => {
    expect(
      applyDownloadEvent(EMPTY_PROGRESS, { type: 'started', contentLength: 0 }).totalBytes,
    ).toBeNull();
    expect(
      applyDownloadEvent(EMPTY_PROGRESS, { type: 'started', contentLength: -5 }).totalBytes,
    ).toBeNull();
    expect(
      applyDownloadEvent(EMPTY_PROGRESS, {
        type: 'started',
        contentLength: Number.NaN,
      }).totalBytes,
    ).toBeNull();
  });

  it('accepts Progress before Started', () => {
    const next = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'progress',
      chunkLength: 50,
    });

    expect(next.receivedBytes).toBe(50);
    expect(next.totalBytes).toBeNull();
  });

  it('accumulates multiple Progress events', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 100,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 40 });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 20 });

    expect(state.receivedBytes).toBe(60);
    expect(toProgressView(state).percent).toBe(60);
  });

  it('ignores negative and non-finite chunks', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 100,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 10 });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: -3 });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: Number.NaN });

    expect(state.receivedBytes).toBe(10);
  });

  it('clamps percentage to 100', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 100,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 150 });

    expect(toProgressView(state).percent).toBe(100);
  });

  it('marks Finished with known total as 100% without inventing received bytes', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 100,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 90 });
    state = applyDownloadEvent(state, { type: 'finished' });

    expect(state.networkFinished).toBe(true);
    expect(toProgressView(state).percent).toBe(100);
    expect(toProgressView(state).receivedBytes).toBe(90);
  });

  it('marks Finished with unknown total without inventing a percent', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: null,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 40 });
    state = applyDownloadEvent(state, { type: 'finished' });

    expect(state.networkFinished).toBe(true);
    expect(toProgressView(state).percent).toBeNull();
    expect(toProgressView(state).receivedBytes).toBe(40);
  });

  it('resets progress on a new Started event', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 100,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 50 });
    state = applyDownloadEvent(state, { type: 'finished' });
    state = applyDownloadEvent(state, {
      type: 'started',
      contentLength: 200,
    });

    expect(state).toEqual({
      totalBytes: 200,
      receivedBytes: 0,
      networkFinished: false,
    });
  });
});

describe('toCompletedProgressView', () => {
  it('always reports 100% for the install-boundary paint frame', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: null,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 40 });

    const view = toCompletedProgressView(state);
    expect(view.percent).toBe(100);
    expect(view.receivedBytes).toBe(40);
    expect(view.networkFinished).toBe(true);
  });

  it('snaps received bytes up to total when Content-Length was overestimated', () => {
    let state = applyDownloadEvent(EMPTY_PROGRESS, {
      type: 'started',
      contentLength: 200,
    });
    state = applyDownloadEvent(state, { type: 'progress', chunkLength: 90 });

    const view = toCompletedProgressView(state);
    expect(view.percent).toBe(100);
    expect(view.receivedBytes).toBe(200);
    expect(view.totalBytes).toBe(200);
  });
});
