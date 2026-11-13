/**
 * @file Stamps one card for every version of the button and lets a click
 * hold a button open. A version is one entry here and one block in
 * pieces.css under `.v-<id>`.
 */

const VERSIONS = [
  {
    id: 'tint',
    name: 'Tint',
    note: 'The version chosen. A flat fill in the colour of what it names.',
  },
  {
    id: 'bracket',
    name: 'Bracket',
    note: 'A fine square bracket outside each end, standing a little clear.',
  },
  { id: 'bracket-close', name: 'Bracket, close', note: 'The same bracket, hugging the button.' },
  { id: 'bracket-heavy', name: 'Bracket, heavy', note: 'A line twice as thick, standing clear.' },
  {
    id: 'bracket-round',
    name: 'Bracket, heavy and round',
    note: 'Heavy and hugging. Its inner curve runs parallel to the button’s corner.',
  },
  {
    id: 'bracket-flush',
    name: 'Bracket, flush',
    note: 'The same, touching the button. Its inner curve is the button’s own corner.',
  },
];

const template = document.querySelector('#card');
const pieceTemplate = document.querySelector('#piece');
const versions = document.querySelector('#versions');

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
 * Returns a copy of the piece in `state`. The live one answers the pointer
 * and the keyboard. Any other is a picture of one state, and answers neither.
 * @param {HTMLTemplateElement} source
 * @param {string} state
 * @returns {HTMLElement | null}
 */
function pieceIn(source, state) {
  const piece = source.content.firstElementChild?.cloneNode(true);
  if (!(piece instanceof HTMLElement)) return null;
  const button = piece.querySelector('.btn');
  if (!(button instanceof HTMLButtonElement)) return null;
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

if (
  template instanceof HTMLTemplateElement &&
  pieceTemplate instanceof HTMLTemplateElement &&
  versions !== null
) {
  for (const [index, version] of VERSIONS.entries()) {
    const card = template.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceIn(pieceTemplate, slot.getAttribute('data-piece') ?? 'rest');
      if (piece !== null) slot.replaceWith(piece);
    }
    versions.append(card);
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
