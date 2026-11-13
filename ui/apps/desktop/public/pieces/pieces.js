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
 * @property {'type' | 'list-settled' | 'head' | 'button' | 'function' | 'seam'} kind What its
 *   card shows.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'type-block',
    name: 'Block',
    note: 'A block like any other at rest. Typing empties its fill and rings it in its own colour.',
    kind: 'type',
  },
  {
    id: 'type-well',
    name: 'Well',
    note: 'A recess, always: what is typed sits in the surface, what is picked sits on it.',
    kind: 'type',
  },
  {
    id: 'type-line',
    name: 'Line',
    note: 'No fill, a dashed line beneath, as a blank to be written on.',
    kind: 'type',
  },
  {
    id: 'head',
    name: 'Step head',
    note: 'Bracketed like a function, in the verb’s colour: the verb, then the name it makes.',
    kind: 'head',
  },
  {
    id: 'list',
    name: 'Values',
    note: 'Each value is one more block a hair apart. Point at one and the run parts around it. Point at a bracket and the function opens.',
    kind: 'list-settled',
  },
  {
    id: 'tint',
    name: 'Button',
    note: 'A column. A flat fill in the colour of what it names.',
    kind: 'button',
  },
  {
    id: 'fn',
    name: 'Function',
    note: 'Bracketed, in two blocks a hair apart: the function, then its field.',
    kind: 'function',
  },
  {
    id: 'seam',
    name: 'Seam',
    note: 'A heavy chevron in a darker shade of the brackets’ colour.',
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

/** The template each kind of card is stamped from. */
const CARDS = {
  type: '#card-type',
  'list-settled': '#card-list',
  head: '#card-head',
  seam: '#card-seam',
  button: '#card',
  function: '#card',
};

/** The function the Function card shows. */
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
 * @typedef {object} Spec What a piece is to be.
 * @property {string | null} state `live`, or the one state it is a picture of.
 * @property {string | null} name What a column is called, where it is not `dcn`.
 * @property {string | null} fn The function it is, or null for a column.
 * @property {string[]} list The columns a function takes as a list, if it takes one.
 * @property {string | null} typed A value the function takes that is typed, as `kind:text`.
 */

/**
 * Reads what the piece for `slot` is to be.
 * @param {Element} slot
 * @param {Version} version
 * @returns {Spec}
 */
function specOf(slot, version) {
  // A slot in a row says what it is. The others are the piece the version is about.
  const inRow = slot.closest('.row') !== null;
  const shown = version.kind === 'function' ? SHOWN_FUNCTION : null;
  const list = slot.getAttribute('data-list');
  return {
    state: slot.getAttribute('data-piece'),
    name: slot.getAttribute('data-name'),
    fn: inRow ? slot.getAttribute('data-fn') : shown,
    list: list === null ? [] : list.split(','),
    typed: slot.getAttribute('data-type'),
  };
}

/**
 * Makes `piece` a function: bracketed, its name in the first segment, and
 * after it either the list it takes or the one field it works on.
 * @param {HTMLTemplateElement} source
 * @param {HTMLElement} piece
 * @param {HTMLButtonElement} button
 * @param {Spec} spec
 * @param {string} fn
 */
function makeFunction(source, piece, button, spec, fn) {
  piece.classList.add('fn', 'is-bracketed');
  button.textContent = fn;
  if (spec.list.length > 0) {
    const list = document.createElement('span');
    list.className = 'list';
    for (const name of spec.list) {
      const item = pieceFor(source, { state: spec.state, name, fn: null, list: [], typed: null });
      if (item !== null) list.append(item);
    }
    button.after(list);
    return;
  }
  if (spec.typed !== null) {
    const [kind = 'text', text = ''] = spec.typed.split(':');
    const block = document.createElement('label');
    block.className = `type is-${kind}`;
    const input = document.createElement('input');
    input.value = text;
    input.spellcheck = false;
    input.setAttribute('aria-label', kind === 'number' ? 'A whole number' : 'A text value');
    block.append(input);
    button.after(block);
    return;
  }
  const field = FIELDS.get(fn);
  if (field === undefined) return;
  const segment = document.createElement('button');
  segment.type = 'button';
  segment.className = field.kind === 'value' ? 'btn field is-value' : 'btn field';
  segment.textContent = field.text;
  button.after(segment);
}

/**
 * Returns a copy of the piece that `spec` describes. A live one answers the
 * pointer and the keyboard. Any other is a picture of one state, and answers
 * neither.
 * @param {HTMLTemplateElement} source
 * @param {Spec} spec
 * @returns {HTMLElement | null}
 */
function pieceFor(source, spec) {
  const piece = source.content.firstElementChild?.cloneNode(true);
  if (!(piece instanceof HTMLElement)) return null;
  const button = piece.querySelector('.btn');
  if (!(button instanceof HTMLButtonElement)) return null;
  if (spec.name !== null) button.textContent = spec.name;
  if (spec.fn !== null) makeFunction(source, piece, button, spec, spec.fn);

  if (spec.state === 'live') {
    for (const segment of piece.querySelectorAll('.btn')) segment.classList.add('live');
    return piece;
  }
  for (const control of piece.querySelectorAll('button')) control.tabIndex = -1;
  if (spec.state === 'hover') {
    piece.classList.add('is-hover');
    button.classList.add('is-hover');
  }
  if (spec.state === 'pressed') button.classList.add('is-active');
  if (spec.state === 'open') button.setAttribute('aria-expanded', 'true');
  if (spec.state === 'focus') button.classList.add('is-focus');
  if (spec.state === 'off') {
    piece.classList.add('is-off');
    for (const control of piece.querySelectorAll('button')) control.disabled = true;
  }
  return piece;
}

if (pieceTemplate instanceof HTMLTemplateElement) {
  for (const [index, version] of VERSIONS.entries()) {
    const template = document.querySelector(CARDS[version.kind]);
    if (!(template instanceof HTMLTemplateElement)) continue;
    const card = template.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    card.querySelector(version.kind === 'button' ? '.row-function' : '.row-button')?.remove();
    // The buttons written out in a row answer a click as the stamped pieces do.
    for (const part of card.querySelectorAll('.row .body > .btn')) part.classList.add('live');
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceFor(pieceTemplate, specOf(slot, version));
      if (piece !== null) slot.replaceWith(piece);
    }
    const section = version.kind === 'type' ? '#versions-type' : '#versions-settled';
    document.querySelector(section)?.append(card);
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

/**
 * Keeps a typed block as wide as what is in it, and marks it when what is
 * typed cannot be taken: a number that is not whole, or a name left empty.
 * @param {HTMLInputElement} input
 */
function tend(input) {
  input.style.width = `${Math.max(input.value.length, 1)}ch`;
  const block = input.closest('.type');
  if (block === null || block.classList.contains('is-wrong')) return;
  const wholeNumber = /^-?\d+$/.test(input.value);
  const wrong =
    (block.classList.contains('is-number') && !wholeNumber) ||
    (block.classList.contains('is-name') && input.value.trim() === '');
  input.setAttribute('aria-invalid', String(wrong));
}

for (const input of document.querySelectorAll('.type input')) {
  if (input instanceof HTMLInputElement) tend(input);
}

document.addEventListener('input', (event) => {
  if (event.target instanceof HTMLInputElement) tend(event.target);
});

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closeOthers(null);
  // Enter ends the typing, as leaving the block does.
  if (event.key === 'Enter' && event.target instanceof HTMLInputElement) event.target.blur();
});
