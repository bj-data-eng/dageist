import { describe, expect, it } from 'vitest';

import { isBrowserKey } from './native-feel';
import type { KeyPress } from './native-feel';

/** A press of `key` with the modifiers named. */
function press(key: string, held: Partial<Omit<KeyPress, 'key'>> = {}): KeyPress {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

describe('the browser keys the application window turns off', () => {
  it('include reload, alone and with Command or Control', () => {
    expect(isBrowserKey(press('F5'), true)).toBe(true);
    expect(isBrowserKey(press('r', { metaKey: true }), true)).toBe(true);
    expect(isBrowserKey(press('R', { metaKey: true, shiftKey: true }), true)).toBe(true);
  });

  it('include print, find, saving the page and viewing its source', () => {
    for (const key of ['p', 'f', 's', 'u']) {
      expect(isBrowserKey(press(key, { metaKey: true }), true)).toBe(true);
    }
  });

  it('include going back and forward', () => {
    expect(isBrowserKey(press('ArrowLeft', { altKey: true }), true)).toBe(true);
    expect(isBrowserKey(press('[', { metaKey: true }), true)).toBe(true);
    expect(isBrowserKey(press('BrowserBack'), true)).toBe(true);
  });

  it('leave editing keys alone', () => {
    for (const key of ['c', 'v', 'x', 'z', 'a']) {
      expect(isBrowserKey(press(key, { metaKey: true }), true)).toBe(false);
    }
  });

  it('leave the side panel’s Command B alone', () => {
    expect(isBrowserKey(press('b', { metaKey: true }), true)).toBe(false);
  });

  it('take Control as Command away from a Mac', () => {
    expect(isBrowserKey(press('r', { ctrlKey: true }), false)).toBe(true);
    expect(isBrowserKey(press('r', { metaKey: true }), false)).toBe(false);
  });

  it('leave Control with a letter to move the cursor on a Mac', () => {
    for (const key of ['f', 'p', 'g']) {
      expect(isBrowserKey(press(key, { ctrlKey: true }), true)).toBe(false);
    }
  });

  it('leave letters typed without Command or Control alone', () => {
    expect(isBrowserKey(press('r'), true)).toBe(false);
    expect(isBrowserKey(press('ArrowLeft'), true)).toBe(false);
  });
});
