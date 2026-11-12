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

/** Every command the view can send to the host. */
export interface Host {
  /** Asks the core who it is. */
  readonly coreIdentity: () => Promise<IdentityView>;
  /** Opens a working window of `keys` whole keys of the sample table, held in memory. */
  readonly openSampleWindow: (keys: number) => Promise<TableView>;
  /** Opens the whole sample table where it is stored, without loading it. */
  readonly openSampleResult: () => Promise<TableView>;
  /** Fetches a block of the open table as the bytes of an Arrow IPC stream. */
  readonly fetchTableBlock: (request: BlockRequest) => Promise<ArrayBuffer>;
}
