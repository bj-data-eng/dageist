# How-To

These procedures assume [getting started](getting-started.md) succeeded. Run
commands from the repository root. [AGENTS.md](../AGENTS.md#command-inventory)
owns command selection, concurrency and host-monitoring policy.

## Check The Core

The core has no host or view dependency, so it builds and tests quickly:

```sh
cargo test --offline --locked -p dageist --lib
cargo clippy --offline --locked -p dageist --all-targets --no-deps -- -F unsafe-code -D warnings
```

The tests pass and Clippy reports no warnings. `-F unsafe-code` rejects any
`unsafe` code in the checked package.

## Check The View

Type-check, lint and format-check the view and its Vite configuration, then
bundle the view into `ui/dist/`:

```sh
npm --prefix ui run check
npm --prefix ui run lint
npm --prefix ui run format:check
npm --prefix ui run build
```

All four commands exit successfully. The lint prints nothing when it finds no
problems; it treats warnings as errors and also reports suppression comments
that no longer suppress anything. `format:check` reports files whose formatting
differs from the formatter's output without changing them. `ui/dist/` then
holds `index.html` and an `assets/` folder with the bundled script and
stylesheet.

To restore `ui/node_modules/` from the local npm cache without network access,
run `npm --prefix ui ci --offline`.

## Work On The View In A Browser

Start Vite's development server and open `http://localhost:5173`:

```sh
npm --prefix ui run dev
```

The page updates as files under `ui/` change, with no build of the application.
Outside Tauri there is no engine, so the shell loads a stand-in host: the step
editor works as usual, and the table shows made-up rows. The status badge reads
`Browser preview made-up data`. Stop the server with Ctrl+C.

## Format The View

When `format:check` reports a file, let the formatter rewrite it:

```sh
npm --prefix ui run format
```

The formatter owns all layout in `ui/`, including the JSON, CSS and HTML
files. Do not adjust its output by hand. Run `format:check` again to confirm
that nothing is reported.

## Suppress A Lint Finding

Fix the code when the linter reports a problem. When a finding is a deliberate
exception, suppress that one rule on that one line and state the reason:

```ts
// The menu opens only after the click handler has set `selection`.
// oxlint-disable-next-line typescript/no-non-null-assertion
const node = selection!;
```

Do not disable a rule for a whole file, and do not relax
[ui/.oxlintrc.json](../ui/.oxlintrc.json) to clear a single finding.

## Check The Host

The host embeds `ui/dist/` at compile time, so [check the view](#check-the-view)
first:

```sh
cargo test --offline --locked -p dageist-tauri
cargo clippy --offline --locked -p dageist-tauri --all-targets --no-deps -- -F unsafe-code -D warnings
```

The tests prove the `core_identity` command returns the core's identity and
serializes it in the shape the view expects. They also prove that opening the
sample table generates it only when it is missing, and that `table_block`
answers through Tauri's IPC layer with raw Arrow bytes, for a working window
and for a stored result, or with a message. They prove that asking twice for
the steps window opens it once, and that the capability names its label. They
run against Tauri's mock runtime and do not open a real window.

## Open The Sample Table

The `Table` workspace shows a generated claims table of about two million rows
and 300 columns. Build the view, then run the application optimised, because
an unoptimised Polars is many times slower:

```sh
npm --prefix ui run build
cargo run --release --offline --locked -p dageist-tauri
```

The first time the workspace opens, the host writes the sample, about 2 GB of
Parquet, into a `claims-…` folder in the application cache
(`~/Library/Caches/io.github.bj-data-eng.dageist/` on macOS). Later runs reuse
it. Delete the folder to reclaim the space; it is generated again when needed.

The workspace has two modes. `Working window` loads every row of the first 500 keys
into memory. `Whole result` leaves the table in its Parquet files and reads
each block from disk as you scroll.

To time the engine without the application, run
`cargo run --release --offline --locked -p dageist-engine --example measure`.

## Check Formatting

```sh
cargo fmt --all -- --check
```

The command prints nothing and exits successfully when all Rust sources are
formatted.

## Regenerate The Application Icons

`crates/dageist-tauri/icons/icon.svg` is the source of every raster icon. After
editing it, regenerate the icons with the Tauri 2 command-line tool, installed
as the `cargo tauri` subcommand. Only this procedure needs it. Unlike the other
procedures, it runs from the host's directory:

```sh
cd crates/dageist-tauri
cargo tauri icon icons/icon.svg -o icons
```

The tool also writes mobile and Windows Store icons that the desktop bundle
does not use. Remove them so `icons/` keeps only `icon.svg`, `icon.png`,
`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns` and `icon.ico`:

```sh
rm -rf icons/android icons/ios icons/64x64.png icons/Square*.png icons/StoreLogo.png
```
