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
 * @property {'list' | 'head' | 'button' | 'function' | 'seam'} kind The piece it is a version of.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'list-segments',
    name: 'Segments',
    note: 'Each value is one more block a hair apart. A pair is two blocks that touch.',
    kind: 'list',
  },
  {
    id: 'list-brackets',
    name: 'Inner brackets',
    note: 'The values stand in fine marks of their own: square for a list, curly for a dictionary.',
    kind: 'list',
  },
  {
    id: 'list-tray',
    name: 'Tray',
    note: 'The values lie in a recess in the function’s body, a little smaller than it.',
    kind: 'list',
  },
  {
    id: 'list-run',
    name: 'Run',
    note: 'The values are written in one block as they are in code, with commas and colons.',
    kind: 'list',
  },
  {
    id: 'head-call',
    name: 'Bracketed',
    note: 'The format of a function: bracketed, the verb then the name. Its side panels remove the step or add one after it.',
    kind: 'head',
  },
  {
    id: 'head-words',
    name: 'Words',
    note: 'Today’s head: the verb as coloured text, the name underlined, a typed equals sign.',
    kind: 'head',
  },
  {
    id: 'head-block',
    name: 'Block',
    note: 'The verb and the name as two blocks a hair apart, as a function and its field are.',
    kind: 'head',
  },
  {
    id: 'head-flag',
    name: 'Flag',
    note: 'The verb as a solid flag whose point locks into the name, echoing the chevron.',
    kind: 'head',
  },
  {
    id: 'head-rail',
    name: 'Rail',
    note: 'Each step is a strip of its own, with a rail in the verb’s colour down its edge.',
    kind: 'head',
  },
  {
    id: 'head-numbered',
    name: 'Numbered',
    note: 'The step’s number is a block of its own, joined to the verb.',
    kind: 'head',
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
 * @typedef {object} Spec What a piece is to be.
 * @property {string | null} state `live`, or the one state it is a picture of.
 * @property {string | null} name What a column is called, where it is not `dcn`.
 * @property {string | null} fn The function it is, or null for a column.
 * @property {string[]} list The columns a function takes as a list, if it takes one.
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
      const item = pieceFor(source, { state: spec.state, name, fn: null, list: [] });
      if (item !== null) list.append(item);
    }
    button.after(list);
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
    const own = version.kind === 'seam' || version.kind === 'head' || version.kind === 'list';
    const template = document.querySelector(own ? `#card-${version.kind}` : '#card');
    if (!(template instanceof HTMLTemplateElement)) continue;
    const card = template.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    card.querySelector(version.kind === 'button' ? '.row-function' : '.row-button')?.remove();
    if (version.id === 'head-call' || version.kind === 'list') {
      for (const head of card.querySelectorAll('.piece.head')) head.classList.add('is-bracketed');
    }
    // The buttons written out in a row answer a click as the stamped pieces do.
    for (const part of card.querySelectorAll('.row .body > .btn')) part.classList.add('live');
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceFor(pieceTemplate, specOf(slot, version));
      if (piece !== null) slot.replaceWith(piece);
    }
    const sectioned = version.kind === 'head' || version.kind === 'list';
    const section = sectioned ? `#versions-${version.kind}` : '#versions-settled';
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

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closeOthers(null);
});
