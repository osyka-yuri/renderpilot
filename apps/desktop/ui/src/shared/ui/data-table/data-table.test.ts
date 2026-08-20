/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import DataTableTestHost from './data-table.test-host.svelte';

describe('createSvelteTable', () => {
  let target: HTMLDivElement;
  let component:
    | {
        addColumn: () => void;
        addDataRow: () => void;
        getRowIds: () => string[];
        getIdSortState: () => false | 'asc' | 'desc';
        getSorting: () => { id: string; desc: boolean }[];
        getTable: () => object;
        getVisibleCellCount: () => number;
        hideIdColumn: () => void;
        sortByValue: () => void;
        toggleIdSorting: () => void;
        toggleValueSorting: () => void;
      }
    | undefined;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
  });

  it('keeps its table identity while publishing reactive data and columns', () => {
    component = mount(DataTableTestHost, { target });
    flushSync();

    const table = component.getTable();
    expect(component.getRowIds()).toEqual(['two', 'one']);
    expect(component.getVisibleCellCount()).toBe(2);

    component.addDataRow();
    component.addColumn();
    flushSync();

    expect(component.getTable()).toBe(table);
    expect(component.getRowIds()).toEqual(['two', 'one', 'three']);
    expect(component.getVisibleCellCount()).toBe(3);
    expect(target.textContent).toContain('two,one,three');
  });

  it('keeps sorting controlled and publishes the sorted row model reactively', () => {
    const onSortingChange = vi.fn();
    component = mount(DataTableTestHost, { target, props: { onSortingChange } });
    flushSync();

    component.sortByValue();
    flushSync();

    expect(component.getSorting()).toEqual([{ id: 'value', desc: false }]);
    expect(onSortingChange).toHaveBeenCalledWith([{ id: 'value', desc: false }]);
    expect(component.getRowIds()).toEqual(['one', 'two']);
    expect(target.textContent).toContain('one,two');
  });

  it('keeps automatic sorting cycles synchronized with controlled state', () => {
    component = mount(DataTableTestHost, { target });
    flushSync();

    component.toggleValueSorting();
    flushSync();
    expect(component.getSorting()).toEqual([{ id: 'value', desc: true }]);

    component.toggleValueSorting();
    flushSync();
    expect(component.getSorting()).toEqual([{ id: 'value', desc: false }]);

    component.toggleValueSorting();
    flushSync();
    expect(component.getSorting()).toEqual([]);
  });

  it('cycles a string column from descending through none, ascending, and descending', () => {
    component = mount(DataTableTestHost, {
      target,
      props: { initialSorting: [{ id: 'id', desc: true }] },
    });
    flushSync();

    component.toggleIdSorting();
    flushSync();
    expect(component.getSorting()).toEqual([]);
    expect(component.getIdSortState()).toBe(false);
    expect(target.querySelector('output')?.dataset.idSort).toBe('none');

    component.toggleIdSorting();
    flushSync();
    expect(component.getSorting()).toEqual([{ id: 'id', desc: false }]);
    expect(component.getIdSortState()).toBe('asc');
    expect(target.querySelector('output')?.dataset.idSort).toBe('asc');

    component.toggleIdSorting();
    flushSync();
    expect(component.getSorting()).toEqual([{ id: 'id', desc: true }]);
    expect(component.getIdSortState()).toBe('desc');
    expect(target.querySelector('output')?.dataset.idSort).toBe('desc');
  });

  it('reacts to internally owned column visibility while preserving table identity', () => {
    component = mount(DataTableTestHost, { target });
    flushSync();

    const table = component.getTable();
    expect(component.getVisibleCellCount()).toBe(2);
    expect(target.textContent).toContain('two,one:id,value');

    component.hideIdColumn();
    flushSync();

    expect(component.getTable()).toBe(table);
    expect(component.getVisibleCellCount()).toBe(1);
    expect(target.textContent).toContain('two,one:value');
  });
});
