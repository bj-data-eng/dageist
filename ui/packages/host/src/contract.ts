/**
 * @file What the view may ask of the host, and the shapes the host answers
 * in. Features depend on this contract, never on Tauri.
 */

/** Mirrors `IdentityView` in crates/dageist-tauri/src/main.rs. */
export interface IdentityView {
  readonly name: string;
  readonly version: string;
}

/** Mirrors `ColumnView` in crates/dageist-tauri/src/sample_table.rs. */
export interface ColumnView {
  readonly name: string;
  readonly dataType: string;
}

/** Mirrors `TableView` in crates/dageist-tauri/src/sample_table.rs. */
export interface TableView {
  readonly rows: number;
  readonly columns: readonly ColumnView[];
}

/** A block of rows and columns to fetch from the open table. */
export interface BlockRequest {
  readonly offset: number;
  readonly length: number;
  readonly columns: readonly string[];
}

/**
 * What a parameter holds, mirroring `KindView` in
 * crates/dageist-tauri/src/catalogue.rs. Each kind has one drawing.
 */
export type KindView =
  /** A chain; `dtypes` names the dtype groups it may have. */
  | { readonly kind: 'expression'; readonly dtypes: readonly string[] }
  | { readonly kind: 'column' }
  /** A literal typed in; `type` is `integer`, `float`, `string`, `date` or `any`. */
  | { readonly kind: 'value'; readonly type: 'integer' | 'float' | 'string' | 'date' | 'any' }
  | { readonly kind: 'flag' }
  | { readonly kind: 'choice'; readonly words: readonly ChoiceView[] }
  | { readonly kind: 'dtype' }
  | { readonly kind: 'duration' }
  | { readonly kind: 'table' }
  | { readonly kind: 'path' }
  | { readonly kind: 'pairs'; readonly key: KindView; readonly value: KindView };

/** One word of a choice: as Polars spells it, and as it is shown. */
export interface ChoiceView {
  readonly word: string;
  readonly shown: string;
}

/** How many of its kind a parameter holds; `of` names the parameter a per-item one follows. */
export type ShapeView =
  | { readonly shape: 'one' }
  | { readonly shape: 'many' }
  | { readonly shape: 'perItem'; readonly of: string };

/** Mirrors `ParameterView` in crates/dageist-tauri/src/catalogue.rs. */
export interface ParameterView {
  /** Python's name, which identifies the parameter. */
  readonly name: string;
  /** The name shown for it. */
  readonly shown: string;
  readonly label: string;
  readonly kind: KindView;
  readonly shape: ShapeView;
  /** Polars' default as Python writes it, or `null` when it must be given. */
  readonly default: string | null;
}

/** Mirrors `EntryView` in crates/dageist-tauri/src/catalogue.rs. */
export interface EntryView {
  /** The stable identifier saved steps keep, such as `Expr.shift`. */
  readonly id: string;
  readonly applies: 'expression' | 'frame' | 'function';
  /** The key of the family it is swapped within. */
  readonly family: string;
  /** The name shown for it; in English, Polars' own name. */
  readonly name: string;
  readonly hint: string;
  /** The plain head of a step, such as "Add column"; `null` for anything but a step. */
  readonly head: string | null;
  /** The dtype groups an expression method accepts on what it follows. */
  readonly accepts: readonly string[];
  /** Whether it works on a lazy frame without a collect. */
  readonly lazy: boolean;
  readonly parameters: readonly ParameterView[];
}

/** A family or dtype group: its key and the words shown for it. */
export interface WordsView {
  readonly key: string;
  readonly title: string;
}

/** Mirrors `CatalogueView` in crates/dageist-tauri/src/catalogue.rs. */
export interface CatalogueView {
  /** Every family, in the order they are offered. */
  readonly families: readonly WordsView[];
  /** Every dtype group. */
  readonly dtypes: readonly WordsView[];
  /** Every entry, ordered by identifier. */
  readonly entries: readonly EntryView[];
}

/** Mirrors `ViewWindow` in crates/dageist-tauri/src/windows.rs. */
export type WindowName = 'steps';

/** Every command the view can send to the host. */
export interface Host {
  /** Asks the core who it is. */
  readonly coreIdentity: () => Promise<IdentityView>;
  /** Fetches the catalogue of operations the step editor offers, with its English words. */
  readonly operationCatalogue: () => Promise<CatalogueView>;
  /** Opens a working window of `keys` whole keys of the sample table, held in memory. */
  readonly openSampleWindow: (keys: number) => Promise<TableView>;
  /** Opens the whole sample table where it is stored, without loading it. */
  readonly openSampleResult: () => Promise<TableView>;
  /** Fetches a block of the open table as the bytes of an Arrow IPC stream. */
  readonly fetchTableBlock: (request: BlockRequest) => Promise<ArrayBuffer>;
  /** Opens the window called `name`, or brings it forward when it is already open. */
  readonly openWindow: (name: WindowName) => Promise<void>;
  /**
   * Sends `message` to every other window of the application that listens to
   * `topic`. The window that sends it does not receive it.
   */
  readonly broadcast: (topic: string, message: unknown) => void;
  /**
   * Calls `listener` with every message another window sends on `topic`.
   * Returns a function that stops listening.
   */
  readonly listen: (topic: string, listener: (message: unknown) => void) => () => void;
}
