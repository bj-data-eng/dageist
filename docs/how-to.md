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

Expect the tests to pass and Clippy to report no warnings. `-F unsafe-code` rejects any
`unsafe` code in the checked package.

## Check The View

Type-check, lint and format-check the view and its Vite configuration, then
bundle the view into `ui/dist/`:

```sh
npm --prefix ui run check
npm --prefix ui run lint
npm --prefix ui run format:check
npm --prefix ui run build
npm --prefix ui run test
```

All five commands exit successfully. The lint prints nothing when it finds no
problems; it treats warnings as errors and also reports suppression comments
that no longer suppress anything. `format:check` reports files whose formatting
differs from the formatter's output without changing them. `ui/dist/` then
holds `index.html` and an `assets/` folder with the bundled script and
stylesheet. `test` runs Vitest once over the view's data layers: the decoders
in `ui/packages/host`, which read what the host sends and are tested against
the [snapshots written from Rust](#refresh-the-step-report-snapshots) with
expectations written by hand, the bridge host in the same package, against a
stand-in for `fetch`, and the step editor's functions that translate steps
out of the document the host holds, prepare the engine's report for display and
schedule requests, against the same snapshots. React components have no tests. Name a file to run only its tests,
as in `npm --prefix ui run test -- decode-run`.

Add a development dependency with
`npm --prefix ui install --save-dev --save-exact --legacy-peer-deps <package>`.
Without `--legacy-peer-deps`, npm 10 stops with `Cannot read properties of null
(reading 'edgesOut')` while it resolves Vitest's optional peers. The lock file
the flag writes passes `npm ci`.

To restore `ui/node_modules/` from the local npm cache without network access,
run `npm --prefix ui ci --offline`.

## Work On The View In A Browser

The view in a plain browser reaches the real engine through the bridge, a
subcommand of the `dageist` binary that serves the host's commands over local
HTTP. The host embeds `ui/dist/` at compile time, so a fresh checkout cannot
build the binary until the view is built once; [check the view](#check-the-view)
or run:

```sh
npm --prefix ui ci --offline
npm --prefix ui run build
```

Then start the bridge in one terminal, which builds the binary if it is missing,
and Vite's development server in another:

```sh
cargo run --offline --locked -p dageist-tauri -- bridge
npm --prefix ui run dev
```

Open `http://localhost:5173`. The status at the foot of the side panel reads
`dageist 0.1.0` once the page has reached the bridge, and the steps, the
canvas, the node's inputs and the table
show what the application shows. The page updates as files under `ui/` change,
with no build of the application. The bridge prints
`dageist bridge listening on http://127.0.0.1:7878` when it is ready. The
development server forwards the page's requests under `/bridge` to it.

The first request that opens the sample table generates it into the
application's cache folder, as the application does, and a sample the
application generated is reused. The bridge holds the open table for as long as
it runs, so restarting it closes the table the page had open.

When the bridge is not running, the page says so and shows the command above.
Start it and reload the page. To use another port, give both sides the same
one:

```sh
cargo run --offline --locked -p dageist-tauri -- bridge --port 7000
DAGEIST_BRIDGE_PORT=7000 npm --prefix ui run dev
```

The browser opens the steps editor as a second tab where the application opens
a window, and the tabs carry their messages over a broadcast channel. Stop the
bridge and the development server with Ctrl+C. The bridge listens on `127.0.0.1`
only and serves only a page that reaches it through that address or
`localhost`; see [the bridge](explanation.md#the-bridge) for why. It is run from
`cargo run`, a debug build. A release build of the binary has no console on
Windows, because the host sets `windows_subsystem = "windows"` for releases.

## Work On The View In The Application Window

What only the application window has, such as its title bar, the place of its
window controls and the browser keys it turns off, is seen in the window
itself. A debug build can load the view from the development server rather
than from `ui/dist`, so the window updates as files under `ui/` change, as the
browser does. Start the development server, then build and open the window
with a development address set for this build only. `tauri.conf.json` is left
unchanged:

```sh
npm --prefix ui run dev
TAURI_CONFIG='{"build":{"devUrl":"http://localhost:5173"}}' cargo run --offline --locked -p dageist-tauri
```

The window needs no bridge: it reaches the host through Tauri. A change to
the window's own settings in `tauri.conf.json`, or to the permissions in
`capabilities/`, takes effect only once the window is built and opened again.
If the page fails its first load, for example while a file it imports is
missing, the window stays blank, and a later change does not bring it back:
quit the window and open it again.

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
sample table generates it only when it is missing and reports its figures as
text, and that `table_block` answers through Tauri's IPC layer with a
[frame](reference.md#the-block-frame) of row numbers, a report and Arrow bytes,
for a working window and for a stored result, or with a message. They also
prove how a count, a size and a time are written. They prove that asking twice for
the steps window opens it once, and that the capability names its label. They
run against Tauri's mock runtime and do not open a real window.

They also prove the `operation_catalogue` command's view of the catalogue,
and that `ui/packages/host/fixtures/catalogue.json`, the copy of that view the
view's tests decode and the bridge's tests compare its answers with, is current.

The node commands are tested through the same IPC layer, on the transform of
the host's timing corpus (`crates/dageist-tauri/tests/corpus/timing`):
`node_inputs` gives the table the steps start from and each other input with
its columns or dtype; `node_document` gives the document the
host holds, its revision and the report of each step, whether valid, refused at
the link Polars names, incomplete, or not run behind a refused one, with the
dtype after each link and what the tables of allowed combinations say of each
call; `edit_document` makes each of the engine's edits, answers with the edited
document at its next revision or with the engine's refusal as a kind and its
words, and fails as `stale` for a revision that is not the current one;
`rows_after_step` gives the rows after a step of a revision as text, a null as
`null`, and rejects a limit above 1,000 rows, a step that is not in the document,
a step that was not run and a stale revision. Every expected value is written by
hand from the sample's seven rows. The tests also prove that the snapshots in
`ui/packages/host/fixtures/` are current.

The bridge is tested without a socket and through one on a loopback port. The
tests send requests as bytes to the bridge's handling and read the answers
back, for each served command, with the views compared with the snapshots in
`ui/packages/host/fixtures/`. They prove that a request from a `Host` that is
not the bridge's own loopback address and port, one whose body is not sent as
`application/json`, an unknown command, a method other than `POST` and a body
that is not the command's arguments are each refused before a command runs;
that no answer carries a cross-origin header; that the open table is kept
between connections; that the bridge listens on `127.0.0.1` and no other
address; and that the cache folder it uses is the one Tauri names. The command
line's reading of `bridge` and `--port` is tested beside it. The tests do not
start Tauri, and they serve a small sample from a temporary folder.

### Refresh The Catalogue Snapshot

After a change to the catalogue, its language file or the view of it, the
snapshot test fails and says how to refresh
`ui/packages/host/fixtures/catalogue.json`. Write it from the host's own
serializer, then format it as the view's formatter expects:

```sh
DAGEIST_REFRESH_CATALOGUE=1 cargo test --offline --locked -p dageist-tauri catalogue::tests::the_snapshot_is_what_the_command_returns
npm --prefix ui run format
```

The test compares the file with the view by value, so the formatter's
layout does not affect it. Never edit the file by hand.

### Refresh The Table Snapshots

After a change to the views of an opened table or to the frame of a block, the
snapshot test fails and says how to refresh the four `table-*.json` files in
`ui/packages/host/fixtures/`. They are the shape of an opened working window and
of a stored result, at a fixed open time, and a block of each as its frame in
hexadecimal, at a fixed build time:

```sh
DAGEIST_REFRESH_TABLE_SNAPSHOTS=1 cargo test --offline --locked -p dageist-tauri sample_table::tests::the_snapshots_are_what_the_commands_return
npm --prefix ui run format
```

Never edit the files by hand.

### Refresh The Step Report Snapshots

After a change to the timing corpus, the step document or its running, the walk
of the tables, the catalogue's laziness, or the views of what they report, the
snapshot test fails and names the file that differs. Eight files in
`ui/packages/host/fixtures/` are written by one test: `node-inputs.json` from
`node_inputs`, `node-document.json` from `node_document`, the corpus's own
steps as the host holds them with the report of running them,
`step-report.json` from running the corpus's steps,
`step-report-refused.json` from a document whose second step the tables refuse, with
the words they recorded of Polars, and whose third is not run, `step-report-incomplete.json` from one whose second step
holds something still to be picked, which the third waits for,
`step-rows.json` from the rows after the `days_open` step,
`step-report-variants.json`, a report the host builds by hand to hold the
variants no real document gives together, and `vocabulary.json`, the words of
every closed list in a report, an edit and the catalogue, written from the
engine's own lists of variants. The view's tests compare the decoders' word
lists with `vocabulary.json`, so a variant the engine adds makes them fail until
the contract and the decoders name it. Write the files from the host's own
serializer, then format them, then run the view's tests, which decode them:

```sh
DAGEIST_REFRESH_STEP_REPORTS=1 cargo test --offline --locked -p dageist-tauri node::tests::the_snapshots_are_what_the_commands_return
npm --prefix ui run format
npm --prefix ui run test
```

The test compares the files with the views by value. Review the diff: a change
that the decoders refuse is a change of shape the view has to follow, in the
contract and in `ui/packages/host/src/`. Never edit the files by hand.

### Refresh The Graph Snapshot

`ui/packages/host/fixtures/graph.json` is the graph view of the host's timing
corpus, as the `graph` command reports it. Each file's folder is
written as `/sample`, so the snapshot does not hold the folder of the machine
it was written on. After a change to the corpus, to working a graph out
or to the graph view, the host's snapshot test fails. Write the file again,
format it, and run the view's tests, whose decoder and canvas tests read it:

```sh
DAGEIST_REFRESH_GRAPH=1 cargo test --offline --locked -p dageist-tauri graph::tests::the_snapshot_is_what_the_command_returns
npm --prefix ui run format
npm --prefix ui run test
```

### Refresh The Edit Snapshots

After a change to the engine's edits, their requests or the views of the
document and of an answer to an edit,
`document::tests::the_snapshots_are_what_the_commands_return` fails and names
the file that differs. Two files in `ui/packages/host/fixtures/` are written by
it from `edit_document`: `edit-edited.json`, the answer to an edit that was
made, and `edit-refused.json`, the answer to one the engine refuses. Write them
from the host's own serializer, format them, then run the view's tests:

```sh
DAGEIST_REFRESH_EDITS=1 cargo test --offline --locked -p dageist-tauri document::tests::the_snapshots_are_what_the_commands_return
npm --prefix ui run format
npm --prefix ui run test
```

Review the diff and never edit the files by hand.

### Refresh The Offers Snapshots

After a change to the tables of allowed combinations, the rules, the offers or
the views of them, `offers::tests::the_snapshots_are_what_the_commands_return`
fails and names the file that differs. Fourteen files in
`ui/packages/host/fixtures/` are written by it: `offers-start.json`,
`offers-value.json` and `offers-condition.json` from `offers_at`,
`fits-start.json` and `fits-argument.json` from `candidate_fits`,
`typed-expression.json`, `typed-start-and-argument.json`, `typed-whole.json`,
`typed-date.json`, `typed-unread.json` and `typed-sample-values.json` from
`read_typed`, `options.json` from `option_offers`, `step-starts.json` from
`step_starts`, and `offers-grid.json`, every call at every place of every kind,
which the view's grid test runs through the editor's functions to find what the
editor cannot express:

```sh
DAGEIST_REFRESH_OFFERS=1 cargo test --offline --locked -p dageist-tauri offers::tests::the_snapshots_are_what_the_commands_return
npm --prefix ui run format
npm --prefix ui run test
```

Review the diff. When `offers-grid.json` changes, the view's grid test, in
`ui/packages/steps/src/model/grid.test.ts`, may fail: the declared gaps in
`ui/packages/steps/src/model/limits.ts` are to be updated with the change, or
the editor is to draw what the engine allows. Never edit the files by hand.

### Refresh The Census Snapshot

After a change to a variant the step editor has to handle, such as a kind of start, a kind of
parameter, a class of dtype, a cause or rule of a refusal, a state of a step or of its rows or a
kind of input, `report::census::tests::the_snapshot_is_what_the_host_reports` fails and names
`ui/packages/host/fixtures/census.json`. The census lists every such variant from the Rust types:
a type with a list of all its variants is read through it, and any other is listed by
`every_variant!` in `crates/dageist-tauri/src/report/census.rs`, which takes one arm for each
variant by name and expands a `match` with no wildcard arm, so adding a variant fails to compile
until it has an arm. Write the file from there, format it, then run the view's tests, which decode
it:

```sh
DAGEIST_REFRESH_CENSUS=1 cargo test --offline --locked -p dageist-tauri report::census::tests::the_snapshot_is_what_the_host_reports
npm --prefix ui run format
npm --prefix ui run test
```

Review the diff. A variant the census gains that the editor does not draw fails the view's census
test, in `ui/packages/steps/src/model/census.test.ts`, until the editor draws it or
`ui/packages/steps/src/model/limits.ts` declares it as a gap with its reason. Never edit the file by
hand.

### Refresh The Corpus Snapshot

After a change to the catalogue, to the tables of allowed combinations, to what the engine's edits
start a part of a document with, to the walk or to a report, `corpus::tests::the_snapshot_is_what_the_host_reports`
fails and names the file of `ui/packages/host/fixtures/corpus-*.json` that differs. The corpus is
step documents the engine made by its own edits and the report the host gives of each, written by
`crates/dageist-tauri/src/corpus/`: a chain of methods that stands after each shape of value the
engine allows each method after, each function that starts a chain, each kind of start, each step
of the catalogue with each parameter it has, a refusal for each cause and for each rule and stage the
tables give a call, and a chain in each of the 30 contexts. The other tests of the module hold what
it holds against the engine's own tables, so a catalogue entry, a step, a parameter, a context or a
kind of start that no document holds fails there, naming it. Write the files, then run the view's
tests, which rebuild every document:

```sh
DAGEIST_REFRESH_CORPUS=1 cargo test --offline --locked -p dageist-tauri corpus::tests::the_snapshot_is_what_the_host_reports
npm --prefix ui run test
```

The files are written one document to a line, which `npm --prefix ui run format` leaves alone: they
are in the formatter's ignore list in `ui/.oxfmtrc.json`, for indented they would be four times as
long. The run builds the engine's tables of every place once, a context to a thread, which takes
about half a minute. Review the diff. A document the corpus gains that holds a piece the editor
cannot build fails the view's corpus test, in `ui/packages/steps/src/model/corpus.test.ts`, until the
editor draws it or `ui/packages/steps/src/model/limits.ts` declares it as a gap with its reason. Never
edit the files by hand.

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
It serves blocks of the size the grid asks for, 140 rows by 17 columns, from a
table in the folder you name, and generates one there only when it has none.

## Check The Fuzz Framework

`dageist-fuzz` is a workspace member outside `default-members`, so name it. It
does not need the view or the host:

```sh
cargo test --offline --locked -p dageist-fuzz
cargo test --offline --locked -p dageist-fuzz --all-features
cargo clippy --offline --locked -p dageist-fuzz --all-targets --all-features --no-deps -- -F unsafe-code -D warnings
```

Expect the tests to pass and Clippy to report no warnings. The campaign tests that build
isolated source checkouts and run sanitizer-guided targets are ignored by
default; the [crate README](../crates/dageist-fuzz/README.md) describes how to
run them, and the crate's controlled author fixture under
`crates/dageist-fuzz/tests/fixtures/subject/` is a standalone workspace with its
own lock file.

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
rustdoc JSON, with the nightly toolchain the whole workspace uses.

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

`api/polars/python.txt` traces the Python API of the same release. For each
public method, property and operator of `Expr` and its namespaces,
`when`/`then`, `LazyFrame` and `LazyGroupBy`, and each function of
`polars.functions` and `scan_*` function, one line gives:

- its parameters with their defaults;
- the binding methods it calls;
- the public Rust items those reach;
- how it is built: the same name in Rust, another name or several Rust items,
  in Python from other calls, or not resolved to a public Rust item;
- the other Polars calls it is built from, when it has any.

A binding call made through code the item delegates to is counted as the
item's own: private helpers in any module; the engine or resolver object a
helper returns or that the item makes; and class methods such as
`pl.LazyFrame.from_lazyframe_resolver`. Take a flag that defaults to `False`
or `None` and that the function never assigns. Where the call does not pass it,
an `if` that tests it as `flag`, `not flag`, `flag is None` or
`flag is not None` keeps only the branch the default takes. For example,
`if background:` drops its body. Tests that combine the flag with anything else
keep both branches. A call that spreads `*args` or `**kwargs` keeps every
branch. The item's own public methods are not followed; they are listed as what
it is built from.

The header counts each kind and says what is not covered, such as the eager
`DataFrame` and `Series`. The generator reads the Python with Python's own
parser, through `python3` and
[api/generator/python/inventory.py](../api/generator/python/inventory.py), so
no Python package needs installing. The parser must read the syntax Polars is
written in, so `python3` must be at least the oldest Python that Python
Polars supports, 3.11 at this release. It reads the binding in
`vendor/polars/crates/polars-python` with a Rust parser, and the Rust API from
the other Polars audits as they are on disk. `--polars` refreshes those first;
`--crate python` reads them as they are.

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

Without a selector the command refreshes every audit; `--polars` refreshes the
Python audit after the Polars audits it reads. Read the diff and
explain each change from its source change; a Polars upgrade shows every
added, removed and changed item, and every feature that is turned on or off.
Keep the source change and the refreshed audits in one review.

### Check The API Tool

The generator's tests use its own workspace. The workspace's toolchain applies
to it too:

```sh
cargo test --offline --locked --manifest-path api/generator/Cargo.toml
cargo clippy --offline --locked --manifest-path api/generator/Cargo.toml --all-targets -- -F unsafe-code -D warnings
cargo fmt --manifest-path api/generator/Cargo.toml -- --check
```

Three further tests run the pinned Cargo and rustdoc on small workspaces of
their own. One shows that a dependency is documented with the features its
release package turns on and no others. Another shows that the generator
refuses a dependency whose features differ from that build. The third shows
that the release check passes a matching build and names a feature that
differs. Run them when the generator's Cargo or rustdoc handling changes:

```sh
env -u CARGO_TARGET_DIR CARGO_NET_OFFLINE=true cargo test --offline --locked --manifest-path api/generator/Cargo.toml --lib -- --ignored
```

## Add Entries To The Catalogue

A new entry is data in `crates/dageist-engine/src/catalog/entries.rs` and its words in
`en.toml`, with each parameter as the signature in `api/polars/python.txt` has it. Refresh
[the tables](#refresh-the-tables-of-allowed-combinations) and the
[snapshots the host writes](#refresh-the-offers-snapshots), then run the engine's tests, which check
the entry against Polars: its dtypes, its parameters and defaults, its words, and the facts the walk
reads. A method with an expression parameter is checked by
`allowed::tests::broadcast`: for each such parameter it gives Polars a receiver of one value and a
column for the operand, and holds the catalogue's `broadcasts` to whether the result is many values,
so a parameter that Polars does not meet a receiver of one value with each value of, as one that takes
one value only, has to say so with `not_broadcasting`. A test that cannot measure a parameter is
listed there with the reason. A parameter of the [pattern kind](explanation.md#the-pattern-kind) is
given the harness's pattern, one with a named group that every method of text accepts, wherever the
generator of the tables builds the call; a pattern that names the fields of what the method gives, as
`str.extract_groups` does, has its result named by the pattern at the walk and not by the table. A
parameter of the time zone kind is given the zone `UTC` and a required choice its first word, and `dt.strftime` the
format `iso`, which every dtype takes, wherever the generator builds the call; what the zone a call is given
decides of what it gives, a moment in it or in none, is the walk's, as is whether a duration typed into a method that
reads one is a duration Polars reads, which it asks of Polars with the dtype that the method follows.
A method that takes some of the values it follows, so that the data or the arguments decide how many it
gives, is named in `Counting::of` in `crates/dageist-engine/src/catalog/counting.rs` with what decides it, and the
walk works its count out of the arguments in `counted_by` in `crates/dageist-engine/src/document/walk.rs`. A function that
starts an expression with a count its arguments decide, as the ranges and `repeat` do, is named there too, as
`Counting::TheArguments`, and the walk asks `made_by` in `crates/dageist-engine/src/document/ranges.rs` how many values it
gives and what no arguments make of it; its expression parameters that Polars reads as one value say so with
`reading_one_value`, and the generator of the tables lists them in `SCALAR_ONLY`. The
generator of the tables then writes `counted` for it in the dtype and option tables, whatever three rows give, and
measures its context going on to one value. The fuzz target draws such a method, and such a function, only where
`DAGEIST_FUZZ_FOCUS` names it: a campaign without a focus that draws them finds counts that no control settles, as
[the explanation](explanation.md#what-the-tables-are-checked-against) lists.
A method that follows a list or an array is measured on the classes `measured_classes` in
`crates/dageist-engine/src/allowed/table.rs` names: the 26 of `Representative::MEASURED`, which a list or an
array of another element dtype is read as by `Representative::sibling`; those and the lists and arrays of
the core element dtypes, for a method `SENSITIVE` names, whose answer depends on the element dtype; and every
class, for a method of the `list` or the `arr` namespace. The test
`allowed::tests::siblings` runs Polars on every combination that is read as another and fails with the
methods that need to be named, or that are named and need not be, so a new method is held to the claim by
running it. A method that Polars panics on for an element dtype says so in `Rule::forbids_receiver`, which
the tables, the offers and a run of a document read before they ask Polars, and `allowed::tests::panics`
runs the combination in a process of its own to show it.
A new step of a table also needs what a new step starts with, in `start_step` in
`crates/dageist-engine/src/catalog/starting.rs`, and, where it has a flag or a choice, what the generator
of the option table gives it to run on, in `option_step` under `crates/dageist-engine/src/allowed/tests/`.
A step that holds no expression is drawn for itself by the fuzz target, where it is listed among the
`Plain` steps of `allowed_fuzz/generate.rs` with the arguments Polars takes of it.
Last, run the fuzz target for more seeds than the engine's tests do,
[as below](#check-the-tables-against-polars).

## Refresh The Tables Of Allowed Combinations

The files `counts.txt`, `dtypes.txt`, `contexts.txt` and `options.txt` in
[api/catalogue/](../api/catalogue/dtypes.txt) hold what Polars does with every
combination of the catalogue, as
[the explanation](explanation.md#which-combinations-polars-allows) describes.
An engine test makes them again and compares: it fails, naming the file and
some of the lines that differ, after a change to the catalogue, to the
generator under `crates/dageist-engine/src/allowed/tests/`, or to Polars. To
check them, with the rest of the engine:

```sh
cargo test --offline --locked -p dageist-engine
```

To write them again, which takes a few seconds to ten, run the test with the
variable set, then build and test again so that the engine embeds the new files:

```sh
DAGEIST_REFRESH_TABLES=1 cargo test --offline --locked -p dageist-engine allowed::tests::artifacts
cargo test --offline --locked -p dageist-engine
```

Never edit the files by hand. Read the diff and explain each changed line from
its source: a line that was allowed and is refused, or was refused at the
schema and now only at the rows, is a change of what the editor may offer; a
changed reason with the same outcome is Polars rewording. Each context has a
line for the chain that continues and one for the chain that ends, and a
refresh shows them apart: a line that continues and is now allowed is an entry
that may stand where it was refused, and one that ends and is refused is an
entry that may not end an expression there. The header of each file names the tag
of the Python Polars release that `vendor/polars` is at, as `git describe`
gives it there, and its commit, and the counts of allowed and refused
combinations by stage. A refresh is reviewed with the change that caused it.

A named rule is in `crates/dageist-engine/src/allowed/rules.rs`. Adding one
changes the `rules` column of the context table, so refresh the tables with it.
A rule of how a chain ends, which is about a dtype, also says in
`Rule::forbids_end` what it makes of a result whose dtype is not known: it does
not refuse it, and `offer` then reports that whether the chain may end is not
known. A rule about a whole expression, or about the keys of a whole join, such as
`key-is-elementwise` and `join-key-names-are-not-repeated`, is asked by
the walk in `crates/dageist-engine/src/document/walk.rs`, which finds what the
expression holds.

## Check The Tables Against Polars

The tables are measured one combination at a time. A property target in
`crates/dageist-engine/tests/allowed_fuzz/` makes the combinations they were not
measured as and checks them against Polars, as
[the explanation](explanation.md#what-the-tables-are-checked-against) says. It runs
with the engine's tests, from a fixed seed, on 1000 cases, which takes
seconds in a debug build and longer when the machine is short of memory:

```sh
cargo test --offline --locked -p dageist-engine --test allowed_fuzz
```

It is built on [dageist-fuzz](../crates/dageist-fuzz/README.md)'s `property`
feature, which is a dev-dependency of the engine. A case is the dtypes of the
columns `a` and `b` and a step document, written by the document's own writer
under one line that names the dtypes, so a case is plain text:

```text
{"a":"UInt32","b":"String","revision":1}
{"version":1,"steps":[{"id":"s","call":{ … }}]}
```

To run more, name the seed and the number of cases. A campaign runs every case its
seed makes, whatever the cases before it did. It reduces each one that disagrees to the
smallest case that disagrees in the same way, writes that to `target/tmp/allowed-fuzz/`
(or the directory `DAGEIST_FUZZ_KEEP` names) as `<seed>-<its place in the campaign>-<its
length>.case`, and prints them all at the end with how many cases ended each way. Run it
alone, since a case that runs Polars is heavy:

```sh
DAGEIST_FUZZ_SEED=7 DAGEIST_FUZZ_CASES=20000 cargo test --offline --locked -p dageist-engine \
  --test allowed_fuzz a_campaign -- --ignored --nocapture --test-threads=1 --exact
```

A campaign draws its methods evenly, so the more methods the catalogue holds, the
fewer cases of a seed hold any one of them, and the fewer still hold two together. To
check the methods a change is about as deeply as before, focus the campaign on them.
Name them in `DAGEIST_FUZZ_FOCUS`, separated by commas: a method by its name,
`dt.replace`, or by its identifier in the catalogue, `Expr.dt.replace`, every method
of a namespace by a name that ends in a dot, `dt.`, and a function that starts a chain and
takes expressions, such as the ones that make a date or a moment, by its name, `date`, or its
identifier, `polars.date`. A function that takes none, `col` and `lit`, is no focus. A step of a table that
the target draws for itself, as it holds no chain (`reverse`, `head`, `limit`, `tail`, `slice`, `first`,
`last`, `gather_every`, `drop`, `drop_nulls`, `drop_nans`, `fill_nan` and `with_row_index`), is named by
its identifier, `LazyFrame.head`, and `LazyFrame.` names all of them, for a name such as `reverse` is that
of the method of an expression. The pivot, whose aggregate is evaluated against the values of a group and so
starts from `element` most of the time, is named by `LazyFrame.pivot` alone. A campaign focused on steps draws one
of them for six cases of ten.

Every case of a focused campaign then holds at least one of them. Where a chain draws
another method, or draws its start, it draws one of them instead one time in four, and the
rest of the case is drawn as without a focus. A case that still holds none is drawn again, and the tally
counts how many were, and so is a case whose text is longer than the 65536 bytes the target admits. Unset or empty, the variable names no focus, and a seed makes the
same cases as without it. A name that is no method of an expression and no function that
makes a moment is refused, and the refusal names it. A campaign focused on the methods of `dt` and on `replace_strict`:

```sh
DAGEIST_FUZZ_FOCUS=dt.,replace_strict DAGEIST_FUZZ_SEED=7 DAGEIST_FUZZ_CASES=8000 \
  cargo test --offline --locked -p dageist-engine --test allowed_fuzz a_campaign \
  -- --ignored --nocapture --test-threads=1 --exact
```

A batch of new methods lands when focused campaigns on them agree: fresh seeds
focused on the batch's methods of an expression and functions that start a chain, each run at the default number of
threads and again with `POLARS_MAX_THREADS=2 RAYON_NUM_THREADS=2`, for Polars meets
some parts differently on fewer threads. A broad campaign, with no focus, runs
apart from the focused ones, once for each batch or on a schedule. A disagreement it
finds outside the batch is filed as an issue and does not hold the batch back.

Each case is timed. A case that has not ended in `DAGEIST_FUZZ_DEADLINE_MS`
milliseconds, 15000 unless set, ends the campaign: it exits at once with status 75,
which `cargo test` passes on, after it has kept the case as
`<seed>-<place>-timeout.case`, kept the last case in which any thread panicked, which is
the likely cause when a panic of Polars' streaming executor has left it unable to finish
a query (#102), as `<seed>-<place>-panicked.case`, and printed the findings so far, the
number of the case that did not end and the settings to go on from the next one. The
same settings are in `timeout.env` in the kept directory:

```text
the case 4012 did not end in 15000 ms
the case 4012 is kept as target/tmp/allowed-fuzz/00000000002c402a-4012-timeout.case
likely cause: a thread panicked in the case 4009, an earlier case, kept as …-4009-panicked.case
findings so far: 0
to go on from the next case, run again with
DAGEIST_FUZZ_START=4013 DAGEIST_FUZZ_COUNTED=3980
```

Read the two kept cases, replay them if you like, and run again from the next case.
A case's number counts every case the seed makes that is run, including ones that checked
nothing; the cases a focused campaign draws again for holding no method in focus are not
cases and have no number. `DAGEIST_FUZZ_COUNTED` counts only the cases that checked something,
so it is below the number of the case that did not end: in the example 32 of the 4011 cases
before it checked nothing. A case that did not end counts as one that was checked. `DAGEIST_FUZZ_START` makes the cases before it and
passes them over without running them, so the cases from it on are those of a run that began
at the first, and `DAGEIST_FUZZ_COUNTED` makes the run end where that run would have;
without it every case before the start counts, so the run ends earlier than the one it goes
on from by the cases before the start that checked nothing, and a start that is past the end of
the campaign's cases, which would make none, is refused. `--exact` keeps the other tests whose
names contain `a_campaign` out of the run. A
setting that is no whole number (the seed and the number of cases too), a number of cases of
0, a deadline of 0, a count above the cases before the start or above the number of cases,
and a start that makes more cases than a campaign counts are refused, with the variable
named. A timed campaign removes a `timeout.env` that an earlier run left in the kept directory
when it starts, so one that is there after a timed run is that run's. A campaign that draws more
than a million cases again, as one focused on very few methods does, ends as insufficient coverage, which it says, and still tells
what it found. A relative `DAGEIST_FUZZ_KEEP` is
from the root of the repository, which is where these commands are run, and not from the
engine's directory that the tests run in.

A timeout that happens while a case that disagrees is being reduced is kept as
`<seed>-<place>-reduction-timeout.case`, with the case that disagreed kept as it was,
as `<seed>-<place>-unreduced.case`, and told of in the findings so far.

This loop goes on until a campaign ends by itself. It runs in a subshell, so the settings
do not stay in yours, and it follows `DAGEIST_FUZZ_KEEP` if that is set. It stops at any end
that leaves no `timeout.env`, which is a campaign that ran to its last case or one that failed
for another reason, such as a case that disagrees, and leaves every timeout in the kept
directory:

```sh
(
  keep=${DAGEIST_FUZZ_KEEP:-target/tmp/allowed-fuzz}
  case $keep in /*) ;; *) keep=$PWD/$keep ;; esac
  export DAGEIST_FUZZ_KEEP=$keep DAGEIST_FUZZ_SEED=7 DAGEIST_FUZZ_CASES=8000
  rm -f "$keep/timeout.env"
  while :; do
    if [ -f "$keep/timeout.env" ]; then
      set -a; . "$keep/timeout.env"; set +a
      mv "$keep/timeout.env" "$keep/timeout.env.$(date +%s)"
    fi
    cargo test --offline --locked -p dageist-engine --test allowed_fuzz a_campaign \
      -- --ignored --nocapture --test-threads=1 --exact
    [ -f "$keep/timeout.env" ] || break
  done
)
```

A case that disagrees is kept in the directory as `<seed>-<place>-<length>.case` when the run that
found it goes on, so a run that ended at a timeout after one has told it in its findings so far, and the runs
after it end without a failure: list the directory for such files, not only the last status.

The case that is about to run is written to `current.case` in that directory
first. To run one kept case again, and see what the tables predicted and Polars did,
name its file. A kept timeout is timed in the same way and ends the replay with the same
status:

```sh
DAGEIST_FUZZ_REPLAY=$PWD/target/tmp/allowed-fuzz/<file>.case cargo test --offline --locked \
  -p dageist-engine --test allowed_fuzz replay_a_case -- --ignored --nocapture
```

It prints the case, what the tables predicted of it and where, what Polars did and
where, the probe and what Polars said of it where the tables refuse a part and
Polars allows the step, which limits excuse the difference if there is one, and the
outcome. Name the file by its whole path, since the test runs from the crate's
directory.

A disagreement is a finding to settle, not to silence. Read it, run the case in
Polars by hand, and decide which it is:

- **A defect of the engine** (the offers, the rules, the generator of the
  tables): write the test that fails for it in `crates/dageist-engine/src/allowed/tests/`
  first, then correct it, refreshing the tables as
  [above](#refresh-the-tables-of-allowed-combinations) if they change. Keep the
  case as `crates/dageist-engine/tests/allowed_fuzz/regressions/fixed-<what>.case`.
  The normal run replays it and expects it to check something.
- **A part Polars does not evaluate**: the tables refuse a part, and Polars allows
  the step because it never evaluates the part. The target settles it by itself: it
  asks Polars about the part with a probe, and counts the case as refused where
  Polars does not evaluate the part when the probe shows the tables right. The part
  is asked as it is written, and Polars refusing that for the tables' reason, at the
  refused call, settles it. Where that does not, the limits are asked next, and then
  the part apart from what surrounds it: the refused call on a column of the dtype
  that reaches it, and again with each literal it is given a column of its dtype; a
  join's key by itself as the key, and the refused call on a
  column as the key; for a refusal that is the target's own comparison of a
  call's plan with its rows (no refusal of Polars), the call on a column in the
  position the case holds it, which counts only with the evidence that Polars' plan
  and rows differ as the tables say, Polars refuses it there, and a call whose plan
  and rows agree and the column alone are allowed there; and, for a call the tables
  refused without knowing the dtype of an argument, whose reason is Polars' for
  another dtype, the part as written, where Polars refusing it at the refused call
  counts without the reason being compared, and the part before the call alone is
  allowed; and, for a refusal of Polars' own words inside a predicate of a condition that
  has an `otherwise`, which Polars drops where the values of the condition are one
  expression, the case with the last value of each condition made a fill forward of what
  it was, where Polars refusing it for the tables' reason shows that the predicate is one
  it skips, and, for a call judged by its parts as below, refusing it for any reason, with the
  same case allowed once the predicate that holds the refused call is made `false`; and, for a predicate that the rule that a condition's predicate be a `Boolean`
  refused where no column of its dtype can be asked or no column is to be had, as in a
  pivot's aggregate, the condition alone with what follows it removed, where Polars refusing
  it in the predicate, and allowing it with the predicate made `false`, shows that a sort key
  of one value or a fold to `true` is why Polars never evaluated it. A call that the tables
  judge by each of its values beside a companion, `replace`, `replace_strict`, `is_between`, `is_close` and `dt.replace`, counts when Polars refuses
  the part at the call, or in what the call is given, whatever words it uses, for the
  value the tables refused first is not always the one Polars meets; the limit for a
  literal asks the same of the case run again with columns, and for a comparison that
  follows the null literal accepts either the tables' reason or a refusal at the call or
  in what it is given. The replay shows the probe
  and what it showed. Where it does not settle a case, Polars allows the probe,
  refuses it for another reason or at another call, or the evidence is missing, and
  the case is one of the other kinds. Keep a case the probe settles as
  `regressions/stricter-<what it shows>.case`, which the normal run replays and
  expects to say so. Do not add a limit for it; a part Polars skips is not a limit
  of the tables, since the editor holds every part to its types. The probe is in
  `allowed_fuzz/probe.rs`; extend it where a part has no probe, with tests that
  Polars allowing the probe, refusing it for another reason, or refusing it at
  another call settles nothing, and that a refusal that a control meets too does not
  settle a mismatch. A part Polars skips is skipped for a cause, and the same part
  without the cause is evaluated: `allowed_fuzz/masks.rs` holds each cause with a
  case that differs from it in that alone, so that a part is not settled where the
  cause is not there.
- **A count that is the rows or one value by chance**: the tables refuse a count of values that
  the data or the arguments decide (the rules `values-meet-in-count` and `window-input-is-not-counted`,
  or `it cannot end here` for a count), and Polars allows the step because the count is the table's
  rows, or one value, as `head(10)` of three rows or `mode` of a column with one most common value is.
  The target settles it by itself: it reduces the step to the one item that holds the refused place, runs
  that item again on twelve rows, on four rows that tie, and with the number that decides its count made 2,
  and counts the case as stricter by design where Polars refuses one of them for lengths. An item with
  counts in more than one place settles nothing, so report it. A sort key of one value that Polars drops is asked by the probe above,
  where the part alone is refused for lengths, and a part dropped whose count is by chance is asked of
  the part apart from the case. Keep such a case as `regressions/by-design-<what it shows>.case`, which
  the normal run replays and expects to say so. A count that stays the rows with any rows, as the values
  that occur most of the places that sort a column, is no case of this: report it, for the walk could
  know it.
- **A case the tables say they do not cover**, where the tables allow and Polars
  refuses, or Polars evaluates a part and accepts it (a literal it broadcasts, the
  nulls that the sample's rows leave), which the probes do not show: add or
  extend a `Limit` in `allowed_fuzz/limits.rs`, with the sentence that says what
  the tables were not measured on, and say it in
  [the explanation](explanation.md#what-the-tables-are-checked-against). A limit
  excuses a case only where it is the cause, so where the feature it is about may be
  there beside another cause (a literal beside a part Polars skips), gate it on the
  case run again without the feature, as the limits for an `explode` and for a
  literal do, and show it with a case that has the cause and one that differs in
  that alone. A control that cannot be built, or that is the case itself, shows
  nothing, so the limit does not apply. Keep the
  case as `regressions/uncovered-<limit>.case`, which the normal run replays and
  expects the limit to excuse; every limit has one. A limit may have others, filed as
  `regressions/uncovered-<limit>--<what it shows>.case`, with `--also-<limit>` after
  it where another limit explains the case too.
- **A mistake of the generator** (a case that Polars refuses for what no table
  is about, such as a value that depends on the rows): correct
  `allowed_fuzz/generate.rs` and say why in a comment there.
- **The asker differing from the engine** (the finding `asker-differs-from-the-engine`
  of the property `asker-agrees-with-engine`, which is no disagreement between the
  tables and Polars): the target asks Polars about a step the walk refuses through
  `tests/ask_polars/mod.rs`, a copy of what the step engine builds, and for a case whose
  step the walk allows it asks the engine and the asker both and compares whether and where
  Polars refuses it, in what words and the dtype of every expression. The words of a refusal
  of the rows are compared with the values they name left out, and of the schema exactly.
  Read the two answers in the finding, replay the case, and find which of the two builds the
  step another way: the copy, if the engine's builder changed (`document/run.rs`, and the
  patterns and durations of `document/model.rs`), which then follows it; or a refusal of the
  engine that the asker lacks, which belongs in the asker if Polars panics without it. Add
  the case as a test in `allowed_fuzz/observe.rs` with the fix. Do not excuse it with a
  limit: a limit is about the tables, and the finding is about the target.

`DAGEIST_FUZZ_STRICT=1` makes the limits and the probe excuse nothing, which shows
what each one would otherwise report. The tally printed with `--nocapture` counts the
cases a limit excused and the cases refused where Polars does not evaluate the part,
and ends with the totals of the cases that agreed, that both the tables and Polars
refused for different reasons, that a limit excused, that were refused where Polars
does not evaluate the part, that a stricter rule refused and that disagreed, so a
limit that excuses most of what a campaign makes shows. A case refused by both for
another reason is no finding: the allowance is right, but Polars never judged what
the tables refused, so it is not counted as agreement, and its outcome names both
reasons.

## Check The Build Against The Polars Release

The engine builds Polars from `vendor/polars` with the same features, crate
patches, dependency versions and toolchain as the Python Polars release there.
That release is `polars-runtime-32`, the runtime a plain `pip install polars`
installs. The release check compares the two builds:

```sh
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --check-release
```

It prints `dageist-tauri builds the crates of vendor/polars as
polars-runtime-32 does` and exits successfully when the builds match.
Otherwise it prints each difference and fails:

- a toolchain in [rust-toolchain.toml](../rust-toolchain.toml) other than the
  release's, or than the one the generator runs Cargo with;
- a Polars crate built with features the release does not turn on, or
  without features it does;
- a Polars crate only one of the builds compiles;
- a package a Polars crate uses at another version or from another source
  than the release, as when a crate patch in the root
  [Cargo.toml](../Cargo.toml) differs from Polars' own;
- a package Polars' crates use, compiled with features the release does not
  turn on, or without features it does. One program shares one copy of each
  package, so features another part of the program turns on reach Polars too.
  Only the Polars crates both builds compile count as users, not the Python
  binding's, and a package compiled both for the program and for build-time
  code is compared by the features of its copies together.

A few differences are expected, and the generator lists them in its source
with the reason for each:

- the Python binding's own features and crates, which connect Polars to
  Python;
- `objc2-exception-helper`, which a feature Tauri turns on in `objc2` adds;
- features Tauri turns on in packages Polars also uses, such as `url`'s
  `serde`;
- `std` in `ndarray` and the numeric crates it builds on, and the default
  features of `ndarray`, `num-complex` and `num-integer`, which `numpy`, used by
  the Python binding, turns on in the release.

A listed exception that no longer applies is reported too, so the list stays
current.

The check reads both builds for the platform it runs on, so a difference that
only another platform's build has is not covered.

## Move Polars To A New Release

`vendor/polars` holds the Polars source at the commit of one Python Polars
release tag, such as `py-2.2.0`. Python Polars and Polars' Rust crates are
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
`git diff --cached --submodule=short` the commit it replaces. Then bring the
build into line with the new release:

1. Set the toolchain in [rust-toolchain.toml](../rust-toolchain.toml) and the
   generator's `RUSTDOC_TOOLCHAIN` to the one in
   `vendor/polars/rust-toolchain.toml`.
2. Copy the `hashbrown` and `tikv-jemalloc-sys` entries of `[patch.crates-io]`
   in `vendor/polars/Cargo.toml` into the root manifest, and add any new patch
   of a crate dageist builds.
3. List the features the release builds the `polars` crate with, and set them
   in [crates/dageist-engine/Cargo.toml](../crates/dageist-engine/Cargo.toml):

   ```sh
   cd vendor/polars
   cargo tree --offline --locked -p polars-runtime-32 -e normal --prefix none --format '{p}|{f}' | grep '^polars v'
   cd ../..
   ```

4. Run the [release check](#check-the-build-against-the-polars-release). It
   names every remaining difference. A package at another version than the
   release's is moved with
   `cargo update --offline -p <name>@<version> --precise <release version>`.
   A feature the Python binding turns on in a Polars crate directly is added
   to the engine's dependency on that crate, unless it only connects Polars to
   Python.
5. Check the engine's start-up against the new release. Read
   `vendor/polars/crates/polars-python/src/on_startup.rs` beside the engine's
   [start-up module](../crates/dageist-engine/src/startup.rs), which lists what it
   does of the release's start-up and what it leaves out, each with its reason.
   A part the release added that does not only connect Polars to Python is done
   in the module, in the release's order, with a test in
   [startup.rs](../crates/dageist-engine/tests/startup.rs); a part the release
   changed, moved or removed is changed the same way; and each part left out
   still has a reason in the module, whether it only connects Polars to Python
   or is one of the two that do not.
6. Build and test the engine, and [refresh the Polars audits](#refresh-the-audits-after-a-change).
7. [Refresh the tables of allowed combinations](#refresh-the-tables-of-allowed-combinations)
   and review them: Polars decides what each combination does, so a new release
   can allow what was refused and refuse what was allowed. Explain each changed
   outcome, and say whether each named rule still holds what Polars does not.
8. [Refresh the step report snapshots](#refresh-the-step-report-snapshots) and
   review them: they hold Polars' own refusal text, which a new release can
   word differently, and the tables' verdicts on the steps of the sample.
9. A parameter the release renamed, moved or made keyword-only is held to the
   refreshed `api/polars/python.txt` by an engine test: the catalogue's
   keyword-only and starred marks are those of Python's signature.

Update the tag and commit in the [reference](reference.md#vendored-sources),
and name the tag in the commit message.

To confirm that the pinned commit is the release's, compare it with the tag
on the Polars repository:

```sh
git submodule status vendor/polars
git -C vendor/polars ls-remote --tags origin py-2.2.0
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
