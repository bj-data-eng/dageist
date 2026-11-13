/**
 * @file Stamps one card for every version of a piece and lets a click hold a
 * button open. A version is one entry in the list here and one block in
 * pieces.css under `.v-<id>`.
 */

/**
 * @typedef {object} Version
 * @property {string} id What its block in pieces.css is called.
 * @property {string} name What its card is headed.
 * @property {string} note One line on what sets it apart.
 * @property {'button' | 'function' | 'seam'} kind The piece it is a version of.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'tint',
    name: 'Tint',
    note: 'The version chosen. A flat fill in the colour of what it names.',
    kind: 'button',
  },
  {
    id: 'bracket',
    name: 'Bracket',
    note: 'A fine square bracket outside each end, standing a little clear.',
    kind: 'button',
  },
  {
    id: 'bracket-close',
    name: 'Bracket, close',
    note: 'The same bracket, hugging the button.',
    kind: 'button',
  },
  {
    id: 'bracket-heavy',
    name: 'Bracket, heavy',
    note: 'A line twice as thick, standing clear.',
    kind: 'button',
  },
  {
    id: 'bracket-round',
    name: 'Bracket, heavy and round',
    note: 'Heavy and hugging. Its inner curve runs parallel to the button’s corner.',
    kind: 'button',
  },
  {
    id: 'bracket-flush',
    name: 'Bracket, flush',
    note: 'The same, touching the button. Its inner curve is the button’s own corner.',
    kind: 'button',
  },
  {
    id: 'fn',
    name: 'Segments',
    note: 'The version preferred. Two blocks a hair apart: the function, then its field.',
    kind: 'function',
  },
  {
    id: 'seam',
    name: 'Chevron',
    note: 'The version chosen: the weight of the brackets, in a darker shade so it sits back.',
    kind: 'seam',
  },
  {
    id: 'seam-lighter',
    name: 'Chevron, a shade lighter',
    note: 'Halfway back to the brackets’ own colour.',
    kind: 'seam',
  },
  {
    id: 'seam-darker',
    name: 'Chevron, a shade darker',
    note: 'Close to the lines of the card. It is there when looked for.',
    kind: 'seam',
  },
];

/**
 * The field each function shown works on, and whether it is a column or a
 * plain value. A function that is not listed takes no field.
 * @type {Map<string, { text: string, kind: 'column' | 'value' }>}
 */
const FIELDS = new Map([
  ['over', { text: 'dcn', kind: 'column' }],
  ['shift', { text: '1', kind: 'value' }],
  ['sub', { text: 'resolved_at', kind: 'column' }],
]);

/** The function the Function section shows. */
const SHOWN_FUNCTION = 'over';

const pieceTemplate = document.querySelector('#piece');

/**
 * Writes `text` into the part of `card` that `selector` finds.
 * @param {HTMLElement} card
 * @param {string} selector
 * @param {string} text
 */
function fill(card, selector, text) {
  const part = card.querySelector(selector);
  if (part !== null) part.textContent = text;
}

/**
 * Makes `piece` the function `fn`: its name in the first segment and the
 * field it works on in a second.
 * @param {HTMLElement} piece
 * @param {HTMLButtonElement} button
 * @param {string} fn
 */
function makeFunction(piece, button, fn) {
  piece.classList.add('fn');
  button.textContent = fn;
  const field = FIELDS.get(fn);
  if (field === undefined) return;
  const segment = document.createElement('button');
  segment.type = 'button';
  segment.className = field.kind === 'value' ? 'btn field is-value' : 'btn field';
  segment.textContent = field.text;
  button.after(segment);
}

/**
 * Returns a copy of the piece for `slot`, as `version` draws it. A live one
 * answers the pointer and the keyboard. Any other is a picture of one state,
 * and answers neither.
 * @param {HTMLTemplateElement} source
 * @param {Element} slot
 * @param {Version} version
 * @returns {HTMLElement | null}
 */
function pieceFor(source, slot, version) {
  const piece = source.content.firstElementChild?.cloneNode(true);
  if (!(piece instanceof HTMLElement)) return null;
  const button = piece.querySelector('.btn');
  if (!(button instanceof HTMLButtonElement)) return null;

  // A slot in a row says what it is. The others are the piece the version is about.
  const inRow = slot.closest('.row') !== null;
  const shown = version.kind === 'function' ? SHOWN_FUNCTION : null;
  const fn = inRow ? slot.getAttribute('data-fn') : shown;
  const name = slot.getAttribute('data-name');
  if (name !== null) button.textContent = name;
  if (fn !== null) makeFunction(piece, button, fn);
  // A function always has brackets. A column has them only in the versions that try them on it.
  if (fn !== null || (version.kind === 'button' && version.id !== 'tint')) {
    piece.classList.add('is-bracketed');
  }

  const state = slot.getAttribute('data-piece');
  if (state === 'live') {
    for (const segment of piece.querySelectorAll('.btn')) segment.classList.add('live');
    return piece;
  }
  for (const control of piece.querySelectorAll('button')) control.tabIndex = -1;
  if (state === 'hover') {
    piece.classList.add('is-hover');
    button.classList.add('is-hover');
  }
  if (state === 'pressed') button.classList.add('is-active');
  if (state === 'open') button.setAttribute('aria-expanded', 'true');
  if (state === 'focus') button.classList.add('is-focus');
  if (state === 'off') {
    piece.classList.add('is-off');
    for (const control of piece.querySelectorAll('button')) control.disabled = true;
  }
  return piece;
}

if (pieceTemplate instanceof HTMLTemplateElement) {
  for (const [index, version] of VERSIONS.entries()) {
    const template = document.querySelector(version.kind === 'seam' ? '#card-seam' : '#card');
    if (!(template instanceof HTMLTemplateElement)) continue;
    const card = template.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    // A function is drawn with the heavy round bracket.
    if (version.kind !== 'button') card.classList.add('v-bracket-round');
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    card.querySelector(version.kind === 'button' ? '.row-function' : '.row-button')?.remove();
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceFor(pieceTemplate, slot, version);
      if (piece !== null) slot.replaceWith(piece);
    }
    document.querySelector(`#versions-${version.kind}`)?.append(card);
  }
}

/**
 * Closes every open button except `keep`.
 * @param {Element | null} keep
 */
function closeOthers(keep) {
  for (const open of document.querySelectorAll('.btn.live[aria-expanded="true"]')) {
    if (open !== keep) open.setAttribute('aria-expanded', 'false');
  }
}

document.addEventListener('click', (event) => {
  const button = event.target instanceof Element ? event.target.closest('.btn.live') : null;
  closeOthers(button);
  if (button === null) return;
  const open = button.getAttribute('aria-expanded') === 'true';
  button.setAttribute('aria-expanded', String(!open));
});

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closeOthers(null);
});
