/**
 * @file What makes the application window behave like an application and
 * not like a web page: the browser's own menu and keys are turned off. It is
 * used only in the application's window. In a browser, reloading and the
 * browser's menu stay, since the preview is a page being worked on. The
 * stylesheet does the rest in both: see the end of `styles.css`.
 */

/** The parts of a key press that decide whether it is one of the browser's own. */
export interface KeyPress {
  readonly key: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/**
 * The keys that, held with Command on a Mac or Control elsewhere, ask the
 * browser for something a page has: reload, print, find, save the page, view
 * its source, and its history.
 */
const BROWSER_KEYS_WITH_COMMAND = new Set(['r', 'p', 'f', 'g', 's', 'u', '[', ']']);

/** The keys that ask the browser for something on their own: reload, and its find bar. */
const BROWSER_KEYS_ALONE = new Set(['F3', 'F5', 'BrowserBack', 'BrowserForward', 'BrowserRefresh']);

/**
 * Reports whether `press` asks the browser for something only a web page
 * has, which the application's window should not do. Command is the Meta key
 * on a Mac, `onMac`, and Control elsewhere. On a Mac, Control with a letter
 * moves the cursor in a text field, as Control+F moves it forward, so it is
 * left alone. Alt with an arrow goes back or forward.
 */
export function isBrowserKey(press: KeyPress, onMac: boolean): boolean {
  if (BROWSER_KEYS_ALONE.has(press.key)) return true;
  if (press.altKey && (press.key === 'ArrowLeft' || press.key === 'ArrowRight')) return true;
  const command = onMac ? press.metaKey : press.ctrlKey;
  return command && BROWSER_KEYS_WITH_COMMAND.has(press.key.toLowerCase());
}

/** Whether the window runs on a Mac, where Command is the Meta key. */
const ON_MAC = navigator.userAgent.includes('Mac');

/** Reports whether `target` is a place to type, which keeps the browser's menu for copy and paste. */
function isTypingPlace(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target.isContentEditable)
  );
}

/** Keeps the browser's menu from opening, except where text is typed. */
function onContextMenu(event: MouseEvent): void {
  if (!isTypingPlace(event.target)) event.preventDefault();
}

/** Keeps the browser from acting on its own keys. The view's own handlers still see them. */
function onKeyDown(event: KeyboardEvent): void {
  if (isBrowserKey(event, ON_MAC)) event.preventDefault();
}

/**
 * Turns off the browser's own menu, except where text is typed, and the
 * browser's own keys. Returns what turns them back on.
 */
export function behaveLikeAnApplication(): () => void {
  // Capture comes before any handler on the page, so no part of the view sees these first.
  document.addEventListener('contextmenu', onContextMenu, { capture: true });
  document.addEventListener('keydown', onKeyDown, { capture: true });
  return () => {
    document.removeEventListener('contextmenu', onContextMenu, { capture: true });
    document.removeEventListener('keydown', onKeyDown, { capture: true });
  };
}
