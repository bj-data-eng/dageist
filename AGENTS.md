# Dageist Repository Guide

Use `$pisct:coordination` for delivery coordination and the task-appropriate
focused PISCT skill. Follow higher-priority instructions.

## Authority Split

This file owns repository discovery, product boundaries and command inventory.
Crate-local `AGENTS.md` files, when present, supplement it with domain facts and
focused checks; they do not establish separate ownership or delivery workflows.
Keep guides as concise discovery maps. Detailed contracts belong with their
owning documentation/source; load only material relevant to the current task.

Use [dageist-admin](.agents/skills/dageist-admin/SKILL.md) for issues and labels,
pull requests, local working material and pause/resume handoffs. PISCT owns
engineering methods. Apply `$pisct:invariants` to engineering and
`$pisct:rust-modeling` to Rust models and APIs. Dageist-owned Rust prohibits
executable `unsafe` without exception. TypeScript follows the Pragmatic
TypeScript Guidelines (`pragmatic-typescript`); its type check, lint and
format check are required gates for every change under `ui/`. The view is
built with React, Tailwind CSS and Vite.

The coordinator owns issue and label updates, staging, commits, integration and
publication within the authorized scope. Use `$pisct:holistic-review` for final
independent integration review.

## Repository Identity And Ownership

This repository owns the host-independent `dageist` core in `src/`, the
graph model, the dataframe engine, the step document's text form and its
notation, the delta packs, the workspace on disk, the fuzz and grid framework,
the grids and campaign that hold the engine to Polars, and the host adapters
under `crates/`,
the React view in `ui/`, their contracts, manifests, tests, documentation and
shared tooling. Root owns workspace wiring and the core. The Tauri host owns
its window, webview configuration, IPC commands, the bridge that serves the
same commands to a browser and the view it serves. The
fuzz framework owns bounded generation, exhaustive grids, reduction, replay
and failure evidence; the domain cases stay with their consumers. The engine's
are in `dageist-engine-fuzz`, which owns the one way Polars is asked of a
step, the judge of a grid's cell, the world table, the axes derived from the
catalogue and the tables, the grids and the campaign target.

`vendor/polars` is the Polars project's source, pinned to a Python Polars
release tag. This repository owns which release it is pinned to, not its
content; never edit files inside it. The engine builds Polars from it with the
same features, crate patches, dependency versions and toolchain as that
release, checked by the API generator's release check. Moving it to another
release follows [the how-to](docs/how-to.md#move-polars-to-a-new-release).

This repository is public, so an issue or pull request body that names
another project's issue or pull request (`owner/repo#N` or its URL) adds an
entry to that item's timeline, and the merge commit repeats the body as a
second entry. Name it from the one dageist issue that tracks the matter, and
from a pull request body only where it matters. Keep it out of commit messages
and titles.

## Discover The Current Structure

Read the relevant sources below; manifests establish package/dependency facts
and current source establishes behavior.

| Fact | Authoritative source |
| --- | --- |
| Root identity, MSRV, lints, workspace/default members, Polars' crate patches | [Cargo.toml](Cargo.toml) |
| Rust toolchain | [rust-toolchain.toml](rust-toolchain.toml) |
| Rust dependency resolution | [Cargo.lock](Cargo.lock) |
| Host identity and dependencies | [crates/dageist-tauri/Cargo.toml](crates/dageist-tauri/Cargo.toml) |
| Window, bundle, view location and content security policy | [crates/dageist-tauri/tauri.conf.json](crates/dageist-tauri/tauri.conf.json) |
| Webview permissions | [crates/dageist-tauri/capabilities/](crates/dageist-tauri/capabilities/) |
| Engine identity, dependencies and Polars features | [crates/dageist-engine/Cargo.toml](crates/dageist-engine/Cargo.toml) |
| Fuzz framework identity, features and dependencies | [crates/dageist-fuzz/Cargo.toml](crates/dageist-fuzz/Cargo.toml) |
| The mechanics of a grid: axes, cells, judging on threads, `Known` lines and the tally | [crates/dageist-fuzz/src/grid/](crates/dageist-fuzz/src/grid/mod.rs) |
| What holds the engine to Polars: the asker, the judge, the world, the literals, the places and the axes; the grids and the campaign target | [crates/dageist-engine/fuzz/](crates/dageist-engine/fuzz/src/lib.rs), [tests/grids/](crates/dageist-engine/fuzz/tests/grids/main.rs), [tests/allowed_fuzz/](crates/dageist-engine/fuzz/tests/allowed_fuzz/main.rs) |
| Fuzz framework front door, provenance and verification guidance | [crates/dageist-fuzz/src/lib.rs](crates/dageist-fuzz/src/lib.rs), [README](crates/dageist-fuzz/README.md), [crate guide](crates/dageist-fuzz/AGENTS.md) |
| The operations the step editor offers, their parameter kinds and shown words | [crates/dageist-engine/src/catalog/](crates/dageist-engine/src/catalog/mod.rs), its language file [en.toml](crates/dageist-engine/src/catalog/en.toml) |
| The step document's format, its reading, its edits and the running of its steps | [crates/dageist-engine/src/document/](crates/dageist-engine/src/document/mod.rs) |
| The notation of a step document's text form, its tree with line and column spans and its printing, without Polars | [crates/dageist-syntax/src/](crates/dageist-syntax/src/lib.rs); its fuzz target [tests/notation_fuzz/](crates/dageist-syntax/tests/notation_fuzz/main.rs) |
| A step document written as Polars-style text and read back against the catalogue | [crates/dageist-text/src/](crates/dageist-text/src/lib.rs), [read.rs](crates/dageist-text/src/read.rs), [write.rs](crates/dageist-text/src/write.rs) |
| The graph's nodes, references, layout, edits and structural checks, without Polars | [crates/dageist-graph/src/](crates/dageist-graph/src/lib.rs) |
| Git packs whose objects are stored as deltas against bases the caller names, written through gix-pack | [crates/dageist-pack/src/](crates/dageist-pack/src/lib.rs), [delta.rs](crates/dageist-pack/src/delta.rs) |
| A workspace's files, their format and their reading through edits; its commits, a node's history and its packing | [crates/dageist-workspace/src/](crates/dageist-workspace/src/lib.rs), [files.rs](crates/dageist-workspace/src/files.rs), [store.rs](crates/dageist-workspace/src/store.rs) |
| The sample workspace the application opens on, the build step that embeds it and its unpacking into the application's data folder | [samples/orders/](samples/orders/workspace.json), [build.rs](crates/dageist-tauri/build.rs), [sample_workspace.rs](crates/dageist-tauri/src/sample_workspace.rs), [folders.rs](crates/dageist-tauri/src/folders.rs); its test [tests/sample.rs](tests/sample.rs) |
| The session (graph, layout, each transform's document and their revisions), working the graph out, and the session saved to a workspace with each edit's commit message | [src/](src/lib.rs), [session.rs](src/session.rs), [walk.rs](src/walk.rs), [messages.rs](src/messages.rs); its tests' files in [tests/corpus/](tests/corpus/) |
| IPC commands and their serialized views; the graph view and its edits, the file dialog, and the commands on a transform's document by its id | [crates/dageist-tauri/src/](crates/dageist-tauri/src/main.rs), [graph.rs](crates/dageist-tauri/src/graph.rs), [document.rs](crates/dageist-tauri/src/document.rs) |
| The command line and the bridge that serves the commands to a browser | [cli.rs](crates/dageist-tauri/src/cli.rs), [bridge.rs](crates/dageist-tauri/src/bridge.rs) |
| The view's IPC calls, their shapes and the decoders that read what the host sends | [ui/packages/host/src/](ui/packages/host/src/contract.ts) |
| Snapshots of what the host's commands return, which the decoders' tests read | [ui/packages/host/fixtures/](ui/packages/host/fixtures/catalogue.json) |
| The corpus of step documents the host writes for the view's tests, and how it is made | [crates/dageist-tauri/src/corpus/](crates/dageist-tauri/src/corpus/mod.rs), [the explanation](docs/explanation.md#the-corpus-of-step-documents) |
| The view's packages and what each may depend on | each `package.json` under [ui/apps/](ui/apps/desktop/package.json) and [ui/packages/](ui/packages/) |
| The view's bundler and design tokens | [ui/apps/desktop/vite.config.ts](ui/apps/desktop/vite.config.ts), [ui/apps/desktop/src/styles.css](ui/apps/desktop/src/styles.css) |
| View toolchain, scripts and compiler settings | [ui/package.json](ui/package.json), [ui/package-lock.json](ui/package-lock.json), [ui/tsconfig.json](ui/tsconfig.json), [ui/tsconfig.node.json](ui/tsconfig.node.json) |
| View lint rules, formatter settings and test settings | [ui/.oxlintrc.json](ui/.oxlintrc.json), [ui/.oxfmtrc.json](ui/.oxfmtrc.json), [ui/vitest.config.ts](ui/vitest.config.ts) |
| Architecture and integration boundaries | [docs/explanation.md](docs/explanation.md) |
| Documentation navigation and source locations | [README.md](README.md), [docs/reference.md](docs/reference.md) |
| Licensing and dependency attribution | [LICENSE](LICENSE), [NOTICE.md](NOTICE.md) |
| Public API audits, their targets and the pinned rustdoc toolchain | [api/generator/](api/generator/src/lib.rs) |
| Polars release source and the tag it is pinned to | [.gitmodules](.gitmodules), the submodule's commit, [vendored sources](docs/reference.md#vendored-sources) |

When sources disagree, report exact paths and revisions. Do not guess, silently
rewrite another authority, or widen the task to reconcile them.

## Product Boundary

Dageist is a Rust-first visual studio for dataframe pipelines. Users compose
sources, joins, transforms, aggregations and outputs such as tables, charts and
file sinks as a directed acyclic graph on a canvas. Follow the
[architecture explanation](docs/explanation.md).

Rust owns application behavior: the graph model, validation, execution, state
and every value the view displays. Put that behavior in the core or in crates the
core depends on. The core never depends on a host. The Tauri host translates
core contracts into IPC commands and serializable views. The `bridge`
subcommand of the same binary serves those commands to a browser over local
HTTP, through the same functions, without a window; no command's behavior is
written twice. The view owns
presentation and input only: it renders what the host reports, forwards user
intent through IPC commands, holds no authoritative application state and
performs no data processing. The step editor writes its steps as a step
document and shows what the engine reports of it: previews, dtypes and refusals
are the engine's, and the view formats no value.

The view is an npm workspace. The shell in `ui/apps/desktop` is the only place
features are composed. `ui/packages/host` is the only package that knows Tauri
or the bridge's transport.
`ui/packages/design` holds primitive components and knows no domain. Each
feature has its own package, exports one entry file and never imports another
feature. Follow the [view structure](docs/explanation.md#how-the-view-is-divided).
Decoders in `ui/packages/host` read what the host sends and refuse a value of
the wrong shape instead of trusting a type assertion.

Tauri is the interim host. [Surgeist](https://github.com/bj-data-eng/surgeist)
is the intended eventual host, so keep host-specific code inside the host
adapter where a later Surgeist adapter can replace it without core changes.
A capability the view needs that Surgeist lacks is Surgeist's work; report it
to that repository rather than implementing it here.

## Backward Compatibility

Before the root `dageist` public API reaches v1.x.x, do not add or retain
compatibility/migration shims or machinery in any crate, the IPC protocol or
the view. Reaching v1 permits consideration, not automatic authorization.
Apply `$pisct:invariants` for semantic API naming.

## Subagent Models

Use `gpt-6.1-sol` for every subagent and the coordinator, with no automatic
alternatives. Follow `$pisct:coordination` for explicit launch settings,
assignment-specific reasoning effort and review independence.

## Generated Artifacts

Source remains authoritative; never hand-edit generated artifacts.

| Artifact | Owner and procedures |
| --- | --- |
| Raster icons in `crates/dageist-tauri/icons/` | [icon procedure](docs/how-to.md#regenerate-the-application-icons) from `icon.svg` |
| `ui/package-lock.json` | npm, from [ui/package.json](ui/package.json) |
| `ui/dist/` (ignored) | `npm --prefix ui run build` |
| `ui/packages/host/fixtures/catalogue.json` | the host's catalogue view, written by its snapshot test; [refresh procedure](docs/how-to.md#refresh-the-catalogue-snapshot) |
| `ui/packages/host/fixtures/{node-inputs,node-document,step-report,step-report-refused,step-report-incomplete,step-report-variants,step-rows,vocabulary}.json` | the host's node commands' views, written by their snapshot test; [refresh procedure](docs/how-to.md#refresh-the-step-report-snapshots) |
| `ui/packages/host/fixtures/graph.json` | the host's graph view of its timing corpus, `crates/dageist-tauri/tests/corpus/timing`, written by its snapshot test; [refresh procedure](docs/how-to.md#refresh-the-graph-snapshot) |
| `ui/packages/host/fixtures/{edit-edited,edit-refused}.json` | the host's answers to an edit that was made and one the engine refused, written by their snapshot test; [refresh procedure](docs/how-to.md#refresh-the-edit-snapshots) |
| `ui/packages/host/fixtures/{offers-start,offers-value,offers-condition,fits-start,fits-argument,options,offers-grid}.json` | the host's offers commands' views, and the grid of every call at every place from which the view lists what it cannot offer, written by their snapshot test; [refresh procedure](docs/how-to.md#refresh-the-offers-snapshots) |
| `ui/packages/host/fixtures/census.json` | the host's census of every variant the step editor has to handle, written from the Rust types by their snapshot test; [refresh procedure](docs/how-to.md#refresh-the-census-snapshot) |
| `ui/packages/host/fixtures/corpus-{starts,methods,parameters,steps,refused}.json` | the corpus of step documents the engine made by its own edits, each with the report the host gives it, written by the host's snapshot test; [refresh procedure](docs/how-to.md#refresh-the-corpus-snapshot) |
| `crates/dageist-tauri/gen/` (ignored) | `tauri-build`, during any build of `dageist-tauri` |
| `crates/dageist-fuzz/tests/fixtures/subject/Cargo.lock` | Cargo, from the standalone fixture workspace's manifests; refresh it with the fixture's own Cargo command, never through the root workspace |
| `api/catalogue/*.txt` | The engine test that runs the catalogue in Polars; [refresh procedure](docs/how-to.md#refresh-the-tables-of-allowed-combinations) |
| `api/public-api.txt`, `api/crates/*.txt`, `api/polars/*.txt` | [api/generator/](api/generator/src/lib.rs); [audit procedures](docs/how-to.md#check-the-public-api-audits) |
| `api/generator/Cargo.lock` | Cargo, from [api/generator/Cargo.toml](api/generator/Cargo.toml) |

API generation audits the root crate and each `crates/dageist-*` crate that has
a library. It audits the Polars crates whose API the steps are built from, with
the features a build of `dageist-tauri` resolves; a Polars upgrade or a change
of those features is refreshed and reviewed with its source change. It also
traces the Python API of that release through its binding to those crates, in
`api/polars/python.txt`, reading the Python with `python3`. The generator is a
separate workspace with its own lock file. API auditing is explicit
maintenance, separate from normal product checks.

## Verifying A Change

These hold for every change to what the engine allows, refuses or reports,
in every brief, whatever a brief leaves out.

- **A grid judged by Polars is the evidence.** Every rule, and every context
  in which the walk judges a call, is held by a grid in
  [`crates/dageist-engine/fuzz/tests/grids/`](crates/dageist-engine/fuzz/tests/grids/main.rs),
  asserted both ways. A grid is built only from the framework's axes: the
  catalogue's entries, parameters and calls, the classes and the table of
  the world, the literals at the edges of every class and the places an
  expression stands, in `dageist-engine-fuzz`, on the mechanics of
  `dageist_fuzz::grid`. What a grid leaves out of an axis is named on the
  axis with why. Build or extend it before the fix: the grid's failing
  cells are the test-only commit, and the fix follows. Every outcome other
  than agreement is a `Known` line with a true reason, scoped no wider than
  what it names. A change to a grid keeps or widens its coverage, shown by
  its tally before and after. How to write one is in
  [the how-to](docs/how-to.md#hold-a-rule-to-polars-with-a-grid).
- **Campaigns feed the grid.** A fuzz campaign explores how calls combine. Each
  disagreement it finds becomes a grid axis or cell first, then a walk fix or
  a `Known` line. A fuzz limit is only for what a grid cannot hold (what the
  data decide on rows, refusals Polars names in no fixed order, a cause in
  the harness), and always has a Polars control, never a match of the
  refusal's words alone. Campaigns run once at the end of a change, not in
  each round of review.
- **Run what the change touches while working.** Between edits, run the grid
  being extended and the focused tests of the code changed. Run the whole
  suite of each changed package, the host, the view and the API checks once,
  after the last code change.
- **Reviews judge the grid.** A reviewer reads whether the grid's axes cover
  the rule and whether each `Known` line and limit is true and narrow, probes
  Polars where in doubt, and reruns a checkpoint only when its evidence is
  missing, stale or contradictory. A small fix after an accepted review gets a
  review of that fix alone.

## Command Inventory

`default-members = ["."]` limits unqualified Cargo commands to the core.
Product packages share the root lockfile.

Select focused checks through `$pisct:testing`; execute through `$pisct:process`.
No blanket workspace tests. A Cargo command run on its own uses Cargo's default
parallelism. When several Cargo commands run at the same time, give each one
build job with `-j 1`. Monitor `pisct host status` and follow coordination's
pressure guidance. Use separate targets for concurrent worktrees changing the
same packages; reuse an existing target when source/context permit. Keep default
test parallelism unless host status recommends a limit.

The root manifest's profiles: a debug build is optimised lightly, with debug
assertions and overflow checks on, and runs the tests about as fast as a
release build; `--release` builds as Polars builds its wheels and takes the
longest; `--profile measure` is Polars as its release runs, for the tables and
the campaigns.
[The explanation](docs/explanation.md#how-dageist-is-built) gives the reasons
and the measurements.

Core checks from the repository root. The core depends on the engine, so they
build Polars:

```sh
cargo check --offline --locked -p dageist
cargo test --offline --locked -p dageist
cargo clippy --offline --locked -p dageist --all-targets --no-deps -- -F unsafe-code -D warnings
cargo fmt --all -- --check
```

Graph checks from the repository root. The crate does not depend on Polars:

```sh
cargo test --offline --locked -p dageist-graph
cargo clippy --offline --locked -p dageist-graph --all-targets --no-deps -- -F unsafe-code -D warnings
```

Pack checks from the repository root. The crate does not depend on Polars; its
tests run `git` to verify the packs it writes:

```sh
cargo test --offline --locked -p dageist-pack
cargo clippy --offline --locked -p dageist-pack --all-targets --no-deps -- -F unsafe-code -D warnings
```

Syntax checks from the repository root. The crate does not depend on Polars;
its tests include the notation fuzz target, which runs through `dageist-fuzz`:

```sh
cargo test --offline --locked -p dageist-syntax
cargo clippy --offline --locked -p dageist-syntax --all-targets --no-deps -- -F unsafe-code -D warnings
```

Text checks from the repository root. The crate depends on the engine, so they
build Polars. The host's corpus tests also write each document of the corpus as
text and read it back:

```sh
cargo test --offline --locked -p dageist-text
cargo clippy --offline --locked -p dageist-text --all-targets --no-deps -- -F unsafe-code -D warnings
```

Workspace checks from the repository root. The crate does not depend on Polars;
its tests run `git` to check the repositories they make:

```sh
cargo test --offline --locked -p dageist-workspace
cargo clippy --offline --locked -p dageist-workspace --all-targets --no-deps -- -F unsafe-code -D warnings
```

Engine checks from the repository root. The tables are measured, and the
campaign target's fixed seed and kept cases run, in the profile `measure`,
which builds Polars as its release does; the debug run is a stricter pass, as
[the how-to](docs/how-to.md#check-the-tables-against-polars) says:

```sh
cargo test --offline --locked -p dageist-engine
cargo test --offline --locked --profile measure -p dageist-engine --test allowed_fuzz
cargo clippy --offline --locked -p dageist-engine --all-targets --no-deps -- -F unsafe-code -D warnings
```

The engine's grids and campaign, from the repository root. The package
`dageist-engine-fuzz` is a workspace member outside `default-members`, so name
it; it builds Polars. Its tests are the framework's contracts (`framework`),
the grids (`grids`, which run every choice of every axis and take the longest;
`DAGEIST_GRID_SAMPLE` projects their time, `DAGEIST_GRID_SCALE=quick` thins them)
and the campaign target's fixed seed and kept cases (`allowed_fuzz`); the
grids that ask the rows of every cell and the campaigns are ignored and run on
request, as [the how-to](docs/how-to.md#hold-a-rule-to-polars-with-a-grid)
says:

```sh
cargo test --offline --locked -p dageist-engine-fuzz
cargo clippy --offline --locked -p dageist-engine-fuzz --all-targets --no-deps -- -F unsafe-code -D warnings
```

View checks from the repository root. `npm --prefix ui ci --offline` restores
`ui/node_modules/` from the local npm cache:

```sh
npm --prefix ui ci --offline
npm --prefix ui run check
npm --prefix ui run lint
npm --prefix ui run format:check
npm --prefix ui run build
npm --prefix ui run test
```

`test` is Vitest. It tests the view's data layers only, the decoders in
`ui/packages/host` and pure preparation functions, against the fixtures written
from Rust, with expectations written by hand; React components have no tests.
The lint denies warnings and reports unused suppressions.
`npm --prefix ui run format` rewrites files; `format:check` only reports.

Fuzz framework checks from the repository root. The crate is a workspace
member outside `default-members`, so name it. No production crate depends on
it; consumers use it through a dev-dependency or an owner-local `fuzz/` package.
The [crate guide](crates/dageist-fuzz/AGENTS.md) and
[README](crates/dageist-fuzz/README.md) own the campaign and sanitizer
procedures:

```sh
cargo test --offline --locked -p dageist-fuzz
cargo test --offline --locked -p dageist-fuzz --all-features
cargo clippy --offline --locked -p dageist-fuzz --all-targets --all-features --no-deps -- -F unsafe-code -D warnings
```

Host checks embed `ui/dist/`, so build the view first:

```sh
cargo test --offline --locked -p dageist-tauri
cargo clippy --offline --locked -p dageist-tauri --all-targets --no-deps -- -F unsafe-code -D warnings
```

API audit checks from the repository root. The tool's own tests and its
fixtures that run Cargo and rustdoc are in the
[audit procedures](docs/how-to.md#check-the-api-tool):

```sh
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --check
CARGO_NET_OFFLINE=true cargo run --offline --locked --manifest-path api/generator/Cargo.toml -- --check-release
```

Run the release check after any change to a manifest, `Cargo.lock`,
`rust-toolchain.toml` or `vendor/polars`. Polars' crates are built from
`vendor/polars` by path, so Cargo prints their own compiler warnings; they are
Polars', not dageist's, and do not fail the checks.

`cargo run --offline --locked -p dageist-tauri` opens the native application
window and runs until it is closed. It is not a finite check; do not supervise
it as one.

`cargo run --offline --locked -p dageist-tauri -- bridge` serves the host's
commands to a browser on `127.0.0.1` and runs until it is stopped. It is a
long-running process, not a finite check: start it for the browser preview,
bound its lifetime when a task starts it, and stop it afterwards.
