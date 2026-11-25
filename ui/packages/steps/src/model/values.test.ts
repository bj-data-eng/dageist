import { describe, expect, it } from 'vitest';

import { NULL_DASH, cellText, shownOf, valuesLine } from './values';

describe('the values of a column as a line', () => {
  it('are the values the engine wrote, apart by commas', () => {
    expect(valuesLine({ shown: ['2026-03-03', '2026-03-05'], more: 0 })).toBe(
      '2026-03-03, 2026-03-05',
    );
  });

  it('write a null as a dash, since the engine writes no text for it', () => {
    expect(valuesLine({ shown: [null, '2', null], more: 0 })).toBe('—, 2, —');
  });

  it('end in an ellipsis where more values follow those shown', () => {
    expect(valuesLine({ shown: ['1', '2', '3', '4'], more: 3 })).toBe('1, 2, 3, 4, …');
  });

  it('are nothing where there are none', () => {
    expect(valuesLine({ shown: [], more: 0 })).toBe('');
  });
});

describe('a cell of the rows as text', () => {
  it('is the text the engine wrote, whatever it holds', () => {
    expect(cellText('2026-03-03')).toBe('2026-03-03');
    expect(cellText('')).toBe('');
  });

  it('is a dash for a null, since the engine writes no text for it', () => {
    expect(cellText(null)).toBe(NULL_DASH);
  });
});

describe('how what is typed in is shown', () => {
  it('is the words the engine wrote, between quotation marks for text', () => {
    expect(shownOf({ kind: 'integer', shown: '12' })).toEqual({ text: false, shown: '12' });
    expect(shownOf({ kind: 'float', shown: '1.5' })).toEqual({ text: false, shown: '1.5' });
    expect(shownOf({ kind: 'string', value: 'a b' })).toEqual({ text: true, shown: 'a b' });
    expect(shownOf({ kind: 'duration', text: '1mo' })).toEqual({ text: true, shown: '1mo' });
    expect(shownOf({ kind: 'pattern', text: 'a+' })).toEqual({ text: true, shown: 'a+' });
    expect(shownOf({ kind: 'path', text: 'a/b.csv' })).toEqual({ text: true, shown: 'a/b.csv' });
    expect(shownOf({ kind: 'timeZone', zone: 'UTC' })).toEqual({ text: true, shown: 'UTC' });
  });

  it('is nothing where nothing is held, a time zone is none, or the argument is not typed in', () => {
    expect(shownOf(undefined)).toBeNull();
    expect(shownOf({ kind: 'timeZone', zone: null })).toBeNull();
    expect(shownOf({ kind: 'columns', names: ['a'] })).toBeNull();
  });

  it('is nothing for a flag typed in, which the editor does not show yet (#177)', () => {
    expect(shownOf({ kind: 'flag', value: true })).toBeNull();
    expect(shownOf({ kind: 'flag', value: null })).toBeNull();
  });
});
