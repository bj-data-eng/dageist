import { describe, expect, it } from 'vitest';

import { isBrowserKey } from './native-feel';
import type { KeyPress } from './native-feel';

/** A press of `key` with the modifiers named. */
function press(key: string, held: Partial<Omit<KeyPress, 'key'>> = {}): KeyPress {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

describe('the browser keys the application window turns off', () => {
  it('include reload, alone and with Command or Control', () => {
    expect(isBrowserKey(press('F5'))).toBe(true);
    expect(isBrowserKey(press('r', { metaKey: true }))).toBe(true);
    expect(isBrowserKey(press('R', { ctrlKey: true, shiftKey: true }))).toBe(true);
  });

  it('include print, find, saving the page and viewing its source', () => {
    for (const key of ['p', 'f', 's', 'u']) {
      expect(isBrowserKey(press(key, { metaKey: true }))).toBe(true);
    }
  });

  it('include going back and forward', () => {
    expect(isBrowserKey(press('ArrowLeft', { altKey: true }))).toBe(true);
    expect(isBrowserKey(press('[', { metaKey: true }))).toBe(true);
    expect(isBrowserKey(press('BrowserBack'))).toBe(true);
  });

  it('leave editing keys alone', () => {
    for (const key of ['c', 'v', 'x', 'z', 'a']) {
      expect(isBrowserKey(press(key, { metaKey: true }))).toBe(false);
    }
  });

  it('leave the side panel’s Command B alone', () => {
    expect(isBrowserKey(press('b', { metaKey: true }))).toBe(false);
  });

  it('leave letters typed without Command or Control alone', () => {
    expect(isBrowserKey(press('r'))).toBe(false);
    expect(isBrowserKey(press('ArrowLeft'))).toBe(false);
  });
});
