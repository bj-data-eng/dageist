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
into a step document, prepare the engine's report for display and schedule
requests, against the same snapshots. React components have no tests. Name a file to run only its tests,
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

Open `http://localhost:5173`. The status badge reads `dageist 0.1.0` once the
page has reached the bridge, and the steps, the node's inputs and the table
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

They also prove the `operation_catalogue` command's view of the catalogue,
and that `ui/packages/host/fixtures/catalogue.json`, the copy of that view the
view's tests decode and the bridge's tests compare its answers with, is current.

The node commands are tested through the same IPC layer, on the claim-timing
sample node: `node_inputs` gives the table the steps start from and each
other input with its columns or dtype; `run_document` reports each step of a
document, whether valid, refused at the link Polars names, or not run behind a
refused one, with the dtype after each link and what the tables of allowed
combinations say of each call, and reports text that is not a document with
its place instead of failing; `rows_after_step` gives the rows after a step as
text, a null as `null`, and rejects a limit above 1,000 rows, a step that is not
in the document and a step that was not run. Every expected value is written by
hand from the sample's seven rows. The tests also prove that the step report
snapshots in `ui/packages/host/fixtures/` are current.

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

### Refresh The Step Report Snapshots

After a change to the sample node, the step document or its running, the walk
of the tables, the catalogue's laziness, or the views of what they report, the
snapshot test fails and names the file that differs. Eight files in
`ui/packages/host/fixtures/` are written by one test: `node-inputs.json` from
`node_inputs`, `step-document.json`, the claim-timing sample's own steps as the
engine's document, which the view's tests compare the editor's starting steps
with, `step-report.json` from running the claim-timing steps,
`step-report-refused.json` from a document whose second step Polars refuses and
whose third is not run, `step-report-unreadable.json` from text that is not a
document, `step-rows.json` from the rows after the `days_open` step,
`step-report-variants.json`, a report the host builds by hand to hold the
variants no real document gives together, and `vocabulary.json`, the words of
every closed list in a report and in the catalogue, written from the engine's
own lists of variants. The view's tests compare the decoders' word lists with
`vocabulary.json`, so a variant the engine adds makes them fail until the
contract and the decoders name it. Write the files from the host's own
serializer, then format them, then run the view's tests, which decode them:

```sh
DAGEIST_REFRESH_STEP_REPORTS=1 cargo test --offline --locked -p dageist-tauri node::tests::the_snapshots_are_what_the_commands_return
npm --prefix ui run format
npm --prefix ui run test
```

The test compares the files with the views by value. Review the diff: a change
that the decoders refuse is a change of shape the view has to follow, in the
contract and in `ui/packages/host/src/`. Never edit the files by hand.

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

## Refresh The Tables Of Allowed Combinations

The files `dtypes.txt`, `contexts.txt` and `options.txt` in
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
known. A rule about a whole expression, such as `key-is-elementwise`, is asked by
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

To run more, name the seed and the number of cases. A campaign stops at the
first disagreement, reduces it to the smallest case that disagrees in the same
way, writes that to `target/tmp/allowed-fuzz/` (or the directory
`DAGEIST_FUZZ_KEEP` names) and prints it, and prints how many cases ended each
way. Run it alone, since a case that runs Polars is heavy:

```sh
DAGEIST_FUZZ_SEED=7 DAGEIST_FUZZ_CASES=20000 cargo test --offline --locked -p dageist-engine \
  --test allowed_fuzz a_campaign -- --ignored --nocapture --test-threads=1
```

The case that is about to run is written to `current.case` in that directory
first, so a case that does not return is found. To run one kept case again, and
see what the tables predicted and Polars did, name its file:

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
  that reaches it; a join's key by itself as the key, and the refused call on a
  column as the key; and, for a refusal that is the target's own comparison of a
  call's plan with its rows (no refusal of Polars), the call on a column in the
  position the case holds it, which counts only with the evidence that Polars' plan
  and rows differ as the tables say, Polars refuses it there, and a call whose plan
  and rows agree and the column alone are allowed there. The replay shows the probe
  and what it showed. Where it does not settle a case, Polars allows the probe,
  refuses it for another reason or at another call, or the evidence is missing, and
  the case is one of the other kinds. Keep a case the probe settles as
  `regressions/stricter-<what it shows>.case`, which the normal run replays and
  expects to say so. Do not add a limit for it; a part Polars skips is not a limit
  of the tables, since the editor holds every part to its types. The probe is in
  `allowed_fuzz/probe.rs`; extend it where a part has no probe, with tests that
  Polars allowing the probe, refusing it for another reason, or refusing it at
  another call settles nothing, and that a refusal that a control meets too does not
  settle a mismatch.
- **A case the tables say they do not cover**, where the tables allow and Polars
  refuses, or Polars evaluates a part and accepts it (a literal it broadcasts, the
  nulls that the sample's rows leave), which the probes do not show: add or
  extend a `Limit` in `allowed_fuzz/limits.rs`, with the sentence that says what
  the tables were not measured on, and say it in
  [the explanation](explanation.md#what-the-tables-are-checked-against). Keep the
  case as `regressions/uncovered-<limit>.case`, which the normal run replays and
  expects the limit to excuse; every limit has one. A limit may have others, filed as
  `regressions/uncovered-<limit>--<what it shows>.case`.
- **A mistake of the generator** (a case that Polars refuses for what no table
  is about, such as a value that depends on the rows): correct
  `allowed_fuzz/generate.rs` and say why in a comment there.

`DAGEIST_FUZZ_STRICT=1` makes the limits and the probe excuse nothing, which shows
what each one would otherwise report. The tally printed with `--nocapture` counts the
cases a limit excused and the cases refused where Polars does not evaluate the part,
and ends with the totals of the cases that agreed, that a limit excused, that were
refused where Polars does not evaluate the part, that a stricter rule refused and
that disagreed, so a limit that excuses most of what a campaign makes shows.

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
- features Tauri turns on in packages Polars also uses, such as `serde_json`'s
  `raw_value`;
- `std` in `ndarray` and the numeric crates it builds on, and the default
  features of `ndarray`, `num-complex` and `num-integer`, which `numpy`, used by
  the Python binding, turns on in the release.

A listed exception that no longer applies is reported too, so the list stays
current.

The check reads both builds for the platform it runs on, so a difference that
only another platform's build has is not covered.

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
5. Build and test the engine, and [refresh the Polars audits](#refresh-the-audits-after-a-change).
6. [Refresh the tables of allowed combinations](#refresh-the-tables-of-allowed-combinations)
   and review them: Polars decides what each combination does, so a new release
   can allow what was refused and refuse what was allowed. Explain each changed
   outcome, and say whether each named rule still holds what Polars does not.
7. [Refresh the step report snapshots](#refresh-the-step-report-snapshots) and
   review them: they hold Polars' own refusal text, which a new release can
   word differently, and the tables' verdicts on the steps of the sample.

Update the tag and commit in the [reference](reference.md#vendored-sources),
and name the tag in the commit message.

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
