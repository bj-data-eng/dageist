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
Outside Tauri there is no engine, so the shell loads a stand-in host: the steps
work as usual, and the table shows made-up rows. The stand-in opens the editor
as a second browser tab where the application opens a window, and carries the
messages between the two over a broadcast channel. The status badge reads
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

## Check The Public API Audits

The files under `api/` record the public API of the core, of each library
crate under `crates/` and of the Polars crates the steps are built from, one
line per public item. The generator in `api/generator/` writes them from
rustdoc JSON. It needs the nightly toolchain named in its source,
`nightly-2026-09-01`, which only these procedures use. Install it once with
`rustup toolchain install nightly-2026-09-01`.

To see what is audited, and to check every audit against the current source:

```sh
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --list
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --check
```

The check prints `current` and the path of each audit and exits successfully.
A stale or missing audit makes it fail with `stale API artifacts:` and the
audits' names; it never writes. `--root`, `--polars` or `--crate <name>`
limits either command to the core, the Polars audits or one audit.

A Polars audit starts with the crate's version and, one per line, the features
a build of `dageist-tauri` enables for it. Those are the features a release
is built with, which can be more than
[crates/dageist-engine/Cargo.toml](../crates/dageist-engine/Cargo.toml) names,
because Polars' own crates turn features on for each other. The generator
refuses to write a Polars audit when documenting the crate on its own would
resolve other features than that build.

Cargo's offline mode needs every crate in [Cargo.lock](../Cargo.lock) in the
local cache, including crates for other platforms. If the check reports a
crate it cannot download, fetch the locked crates once with
`cargo fetch --locked`.

### Refresh The Audits After A Change

After a change to the public API of the core or a crate, or after a Polars
upgrade or a change to the Polars features, refresh the affected audits from
the changed source:

```sh
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --crate dageist-engine
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --polars
```

Without a selector the command refreshes every audit. Read the diff and
explain each change from its source change; a Polars upgrade shows every
added, removed and changed item, and every feature that is turned on or off.
Keep the source change and the refreshed audits in one review.

### Check The API Tool

The generator's tests use its own workspace:

```sh
cargo test --offline --locked --manifest-path api/generator/Cargo.toml
cargo clippy --offline --locked --manifest-path api/generator/Cargo.toml --all-targets -- -F unsafe-code -D warnings
cargo fmt --manifest-path api/generator/Cargo.toml -- --check
```

Two further tests run the pinned rustdoc on a small workspace of their own:
one shows that a dependency is documented with the features its release
package turns on and no others, and the other that the generator refuses a
dependency whose features differ from that build. Run them when the
generator's Cargo or rustdoc handling changes:

```sh
env -u CARGO_TARGET_DIR CARGO_NET_OFFLINE=true cargo test --offline --locked --manifest-path api/generator/Cargo.toml --lib -- --ignored
```

## Move Polars To A New Release

`vendor/polars` holds the Polars source at the commit of one Python Polars
release tag, such as `py-2.1.0`. Python Polars and Polars' Rust crates are
released separately and from different commits, so a `py-*` tag is the only
source that matches a Python release. To move to another release, fetch its
tag into the submodule and check it out; `--depth 1` keeps the submodule
without history:

```sh
git -C vendor/polars fetch --depth 1 origin tag py-2.2.0
git -C vendor/polars checkout --detach py-2.2.0
git add vendor/polars
```

`git submodule status` then shows the new commit with the tag beside it, and
`git diff --cached --submodule=short` the commit it replaces. Update the tag
and commit in the [reference](reference.md#vendored-sources), commit the move
on its own, and name the tag in the commit message.

To confirm that the pinned commit is the release's, compare it with the tag
on the Polars repository:

```sh
git submodule status vendor/polars
git -C vendor/polars ls-remote --tags origin py-2.1.0
```

The two commits are the same. A fresh clone fetches no Polars tags, so there
`git submodule status` shows the commit without its tag.

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
