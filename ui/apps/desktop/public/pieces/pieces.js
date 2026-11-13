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
 * @property {boolean} brackets Whether the piece is drawn with brackets.
 * @property {string | null} fn The function the piece is, or null for a column.
 * @property {'up' | 'down' | null} params The panel a function's parameters go in.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'tint',
    name: 'Tint',
    note: 'The version chosen. A flat fill in the colour of what it names.',
    brackets: false,
    fn: null,
    params: null,
  },
  {
    id: 'bracket',
    name: 'Bracket',
    note: 'A fine square bracket outside each end, standing a little clear.',
    brackets: true,
    fn: null,
    params: null,
  },
  {
    id: 'bracket-close',
    name: 'Bracket, close',
    note: 'The same bracket, hugging the button.',
    brackets: true,
    fn: null,
    params: null,
  },
  {
    id: 'bracket-heavy',
    name: 'Bracket, heavy',
    note: 'A line twice as thick, standing clear.',
    brackets: true,
    fn: null,
    params: null,
  },
  {
    id: 'bracket-round',
    name: 'Bracket, heavy and round',
    note: 'Heavy and hugging. Its inner curve runs parallel to the button’s corner.',
    brackets: true,
    fn: null,
    params: null,
  },
  {
    id: 'bracket-flush',
    name: 'Bracket, flush',
    note: 'The same, touching the button. Its inner curve is the button’s own corner.',
    brackets: true,
    fn: null,
    params: null,
  },
  {
    id: 'fn-below',
    name: 'Parameters below',
    note: 'The brackets say it is a call. Its parameters slide down on mouseover.',
    brackets: true,
    fn: 'shift',
    params: 'down',
  },
  {
    id: 'fn-above',
    name: 'Parameters above',
    note: 'The same, with the parameters sliding up.',
    brackets: true,
    fn: 'shift',
    params: 'up',
  },
  {
    id: 'fn-pinned',
    name: 'Parameters below, kept in view',
    note: 'A parameter that is set stays under the name. The rest arrive on mouseover.',
    brackets: true,
    fn: 'shift',
    params: 'down',
  },
];

/**
 * The parameters of each function shown, with the value each has here.
 * @type {Map<string, { name: string, value: string, set: boolean }[]>}
 */
const PARAMETERS = new Map([
  [
    'shift',
    [
      { name: 'n', value: '1', set: true },
      { name: 'fill_value', value: 'None', set: false },
    ],
  ],
  ['over', [{ name: 'partition_by', value: 'dcn', set: true }]],
]);

const cardTemplate = document.querySelector('#card');
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
 * Makes `piece` the function `fn`: its name, and its parameters in the panel
 * `version` puts them in.
 * @param {HTMLElement} piece
 * @param {HTMLButtonElement} button
 * @param {string} fn
 * @param {Version} version
 */
function makeFunction(piece, button, fn, version) {
  piece.classList.add('fn');
  button.textContent = fn;
  const panel = piece.querySelector(version.params === 'up' ? '.flap-up' : '.flap-down');
  if (panel === null) return;
  panel.classList.add('params');
  panel.replaceChildren(
    ...(PARAMETERS.get(fn) ?? []).map((parameter) => {
      const cell = document.createElement('button');
      cell.type = 'button';
      cell.className = parameter.set ? 'param is-set' : 'param';
      const name = document.createElement('i');
      name.textContent = parameter.name;
      cell.append(name, parameter.value);
      return cell;
    }),
  );
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
  const fn = inRow ? slot.getAttribute('data-fn') : version.fn;
  const name = slot.getAttribute('data-name');
  if (name !== null) button.textContent = name;
  if (fn !== null) makeFunction(piece, button, fn, version);
  if (version.brackets && (version.fn === null || fn !== null)) {
    piece.classList.add('is-bracketed');
  }

  const state = slot.getAttribute('data-piece');
  if (state === 'live') {
    button.classList.add('live');
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
    button.disabled = true;
  }
  return piece;
}

if (cardTemplate instanceof HTMLTemplateElement && pieceTemplate instanceof HTMLTemplateElement) {
  for (const [index, version] of VERSIONS.entries()) {
    const card = cardTemplate.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    // A function is drawn with the heavy round bracket.
    if (version.fn !== null) card.classList.add('v-bracket-round');
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    card.querySelector(version.fn === null ? '.row-function' : '.row-button')?.remove();
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceFor(pieceTemplate, slot, version);
      if (piece !== null) slot.replaceWith(piece);
    }
    const section = version.fn === null ? '#versions-button' : '#versions-function';
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
