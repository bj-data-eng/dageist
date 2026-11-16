# Reference

## Packages And Workspace

The root [Cargo.toml](../Cargo.toml) declares `dageist` 0.1.0, Rust edition
2024, MSRV 1.99.1 and the MIT license. It forbids `unsafe` code for every
workspace member through `[workspace.lints.rust]`. The workspace uses
`default-members = ["."]`, excludes `vendor/`, and has one committed root
[Cargo.lock](../Cargo.lock). Its `[patch.crates-io]` applies the crate patches
of the Polars release in `vendor/polars`: Polars' forks of `hashbrown` and
`tikv-jemalloc-sys`. [rust-toolchain.toml](../rust-toolchain.toml) selects
`nightly-2026-09-01`, the toolchain that release is built with.

| Package | Path | Role |
| --- | --- | --- |
| `dageist` | [src/](../src/lib.rs) | Host-independent core library. |
| `dageist-engine` | [crates/dageist-engine/](../crates/dageist-engine/Cargo.toml) | Dataframe engine on Polars, built from `vendor/polars` as the release there is built, with Polars' allocator as the global allocator: sample data, working windows, stored results, Arrow output, the [catalogue of operations](explanation.md#the-catalogue-of-operations) with its English language file, the [step document](explanation.md#the-step-document) and the running of its steps, and the [tables of allowed combinations](explanation.md#which-combinations-polars-allows) with the offers made from them. Not published. |
| `dageist-fuzz` | [crates/dageist-fuzz/](../crates/dageist-fuzz/Cargo.toml) | Fuzz framework, ported from Surgeist's `surgeist-fuzz`: bounded generation, same-failure reduction, replayable failure evidence and process-isolated campaigns. A workspace member outside `default-members`; no production crate depends on it. Its controlled author fixture is a standalone workspace with its own lock file, outside the product workspace. Not published. See its [README](../crates/dageist-fuzz/README.md). |
| `dageist-tauri` | [crates/dageist-tauri/](../crates/dageist-tauri/Cargo.toml) | Tauri 2 host; builds the `dageist` binary. Not published. |
| `dageist-ui` | [ui/](../ui/package.json) | Private npm workspace for the React view; see [View Packages](#view-packages). |
| `dageist-api-generator` | [api/generator/](../api/generator/Cargo.toml) | Writes and checks the [public API audits](#public-api-audits). A workspace of its own with its own lock file, outside the product workspace. Not published. |

## Vendored Sources

| Path | Source | Pinned to |
| --- | --- | --- |
| `vendor/polars` | [pola-rs/polars](https://github.com/pola-rs/polars), as a shallow Git submodule declared in [.gitmodules](../.gitmodules) | Python Polars `py-2.1.0`, commit `7f902bd3ffd27ff82d14278a09a295ff2f7aed92`. |

The engine builds Polars from the submodule, with the features, crate
patches, dependency versions and toolchain of that release's
`polars-runtime-32`. The
[release check](how-to.md#check-the-build-against-the-polars-release) compares
the two builds.

## Public API Audits

Each audit lists one crate's public API, one line per item, as rustdoc JSON
describes it. The [audit procedures](how-to.md#check-the-public-api-audits)
check and refresh them.

| Audit | Crate |
| --- | --- |
| [api/public-api.txt](../api/public-api.txt) | The core, `dageist`. |
| [api/crates/](../api/crates/dageist-engine.txt) | Each `crates/dageist-*` crate that has a library. The host has only a binary and is not audited. |
| [api/polars/](../api/polars/polars-plan.txt) | `polars`, `polars-lazy` and `polars-plan`, with the version, the `vendor/polars` commit they are built from and the features a build of `dageist-tauri` resolves for each. `polars-plan` holds expressions and their functions, `polars-lazy` the lazy frame, and `polars` the preludes that re-export them. |
| [api/polars/python.txt](../api/polars/python.txt) | The Python API of the same release, each public method, property, operator and function traced through the binding to the Rust items it reaches, with a count of how many keep the Rust name, use another, are written in Python, or are not resolved. |

## Core API

| Item | Returns |
| --- | --- |
| `dageist::crate_name()` | The core's package name, `"dageist"`. |
| `dageist::crate_version()` | The core's package version. |

## IPC Commands

The host registers these commands in
[crates/dageist-tauri/src/main.rs](../crates/dageist-tauri/src/main.rs) and
defines the table commands in
[sample_table.rs](../crates/dageist-tauri/src/sample_table.rs). The view calls
them through the host bridge in
[ui/packages/host/src/](../ui/packages/host/src/tauri.ts).

| Command | Arguments | Result |
| --- | --- | --- |
| `core_identity` | None | `{ "name": string, "version": string }` from the core. |
| `operation_catalogue` | None | The engine's catalogue of operations with its English words: `{ "families", "dtypes", "entries" }`, each entry with its identifier, family, shown name, hint, step head, accepted dtypes, laziness and parameters. The shapes are `CatalogueView` and the types beside it in [catalogue.rs](../crates/dageist-tauri/src/catalogue.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `open_sample_window` | `keys`: how many whole keys to hold in memory, at most 50,000 | `{ "rows": number, "columns": [{ "name": string, "dataType": string }] }` for the working window. |
| `open_sample_result` | None | The same shape for the whole sample, opened where it is stored without loading it. |
| `table_block` | `offset`, `length`: the rows, at most 10,000; `columns`: the column names, each once, or an empty list for all | That block of whichever table is open, as the raw bytes of an Arrow IPC stream, not JSON. Rows past the end are left out. |
| `open_view_window` | `name`: which window, `"steps"` | Nothing. Opens that window, or brings it forward when it is already open. Defined in [windows.rs](../crates/dageist-tauri/src/windows.rs). |

The windows also talk to each other without the core. The host bridge sends a
message as a Tauri event named `dageist:<topic>`, and every other window that
listens to the topic receives it. The steps window and the main window keep
the steps the same this way while the steps are a storyboard in the view.

A table command that fails rejects with `{ "message": string }`; arguments of
the wrong type are rejected by Tauri with its own text. The first call to
either open command generates the sample table, about two million rows of 300
columns and 2 GB of Parquet, into a `claims-…` folder in the application cache
(`~/Library/Caches/io.github.bj-data-eng.dageist/` on macOS). Later calls reuse
it; delete the folder to reclaim the space. The sample is written beside that
folder and moved into place when it is complete, and within one running
application only one command generates it at a time. Two copies of the
application generating at once are not guarded against.

## Host Configuration

[crates/dageist-tauri/tauri.conf.json](../crates/dageist-tauri/tauri.conf.json)
sets the following:

| Setting | Value |
| --- | --- |
| Bundle identifier | `io.github.bj-data-eng.dageist` |
| View | `../../ui/dist`, embedded at compile time; build it with `npm --prefix ui run build` first |
| Global API | `app.withGlobalTauri` is `true`, which injects `window.__TAURI__` |
| Main window | Label `main`, title `dageist`, 1440×900, minimum 960×600, dark theme |
| Steps window | Label `steps`, title `dageist · steps`, 1180×820, minimum 820×520, dark theme. It loads the view's page as `index.html?window=steps` and is opened by `open_view_window`, not at start. It closes when the main window does. |
| Minimum macOS | 13.3, the first version whose system WebKit supports the CSS that Tailwind CSS 4 emits |
| Content security policy | Same-origin scripts, styles and images, plus `data:` images and the IPC channel |

[crates/dageist-tauri/capabilities/default.json](../crates/dageist-tauri/capabilities/default.json)
grants the `main` and `steps` windows the `core:default` permission set, which
includes sending and listening to events.

## View Packages

The view is an npm workspace rooted at [ui/package.json](../ui/package.json).
Every package is private and is consumed as TypeScript source.

| Package | Path | Role | May depend on |
| --- | --- | --- | --- |
| `@dageist/desktop` | [ui/apps/desktop/](../ui/apps/desktop/src/App.tsx) | The shell: picks the host, composes the features, owns the page, the bundler configuration and the design tokens. | Every package below, React, and flechette for the stand-in host |
| `@dageist/host` | [ui/packages/host/](../ui/packages/host/src/contract.ts) | The contract of every IPC command and its implementation over Tauri. | Nothing |
| `@dageist/design` | [ui/packages/design/](../ui/packages/design/src/index.ts) | Primitive components: `Block` (a word that can be pressed), `Piece`, `Slot`, `ManySlot`, `OfferList`, `TypedBlock`, `Popover`, `Hint`, `Peek`, `ToggleButton`, `Panel`, and `blocks.css`, the styles of the words a step is written in. `Popover` is placed by Floating UI. | Floating UI, React, React DOM |
| `@dageist/steps` | [ui/packages/steps/](../ui/packages/steps/src/StepsWorkspace.tsx) | The steps of a transform and their storyboard model: the main window's table with its list of steps, and the window the steps are edited in. | `@dageist/design`, `@dageist/host`, dnd kit, React, React DOM |
| `@dageist/table` | [ui/packages/table/](../ui/packages/table/src/TableViewer.tsx) | The table viewer: fetches blocks as Arrow bytes and draws only those. | `@dageist/design`, `@dageist/host`, flechette, React |

| File | Contents |
| --- | --- |
| [ui/apps/desktop/index.html](../ui/apps/desktop/index.html) | The page Vite builds from. |
| [ui/apps/desktop/vite.config.ts](../ui/apps/desktop/vite.config.ts) | Bundler settings; the bundle is written to `ui/dist/`. |
| [ui/apps/desktop/src/styles.css](../ui/apps/desktop/src/styles.css) | Tailwind entry, the colour and font tokens, and the declarations of the two typefaces. |
| [ui/apps/desktop/public/fonts/](../ui/apps/desktop/public/fonts/) | The typefaces the view is set in, each with the licence it comes under, copied into the bundle as they are: Inter for everything that is not code or data, and JetBrains Mono for code and data. |
| [ui/apps/desktop/src/demo-host.ts](../ui/apps/desktop/src/demo-host.ts) | A stand-in host with made-up rows and the catalogue snapshot, loaded only when the page is open outside Tauri. |
| [ui/apps/desktop/src/catalogue.json](../ui/apps/desktop/src/catalogue.json) | The `operation_catalogue` command's view, written by the host's snapshot test for the stand-in host to serve. Generated; [refresh it](how-to.md#refresh-the-catalogue-snapshot), never edit it. |
| [ui/apps/desktop/public/pieces/](../ui/apps/desktop/public/pieces/index.html) | The Pieces page: a prototype of how the step editor is drawn, in plain HTML, CSS and JavaScript, copied into the bundle as it is and shown in the Pieces tab. |
| [ui/tsconfig.json](../ui/tsconfig.json) | Strict compiler settings for every package's `src/`; emits nothing. |
| [ui/tsconfig.node.json](../ui/tsconfig.node.json) | Strict type checking of the Vite configuration; emits nothing. |
| [ui/.oxlintrc.json](../ui/.oxlintrc.json) | Lint rules: the `correctness` and `suspicious` categories plus the listed rules, all as errors, with type-aware, React and accessibility rules enabled. |
| [ui/.oxfmtrc.json](../ui/.oxfmtrc.json) | Formatter settings: 100-column lines, single quotes, semicolons. |

| npm script | Action |
| --- | --- |
| `dev` | Serves the view at `http://localhost:5173` and updates the page as files change. |
| `check` | Type-checks every package and the Vite configuration without emitting files. |
| `lint` | Lints `ui/` with oxlint, denying warnings and reporting unused suppressions. |
| `format` | Rewrites the files in `ui/` with oxfmt. |
| `format:check` | Reports files that oxfmt would change, without writing. |
| `build` | Bundles the view into `ui/dist/` with Vite. |

Runtime dependencies, bundled into `ui/dist/` and pinned to exact versions in
the package that uses them: React 19.3.1 (`react`, `react-dom`),
[dnd kit](https://dndkit.com) 0.5.3 (`@dnd-kit/react`, `@dnd-kit/dom`) for drag
and drop, and [flechette](https://github.com/uwdata/flechette) 2.7.1 for
reading Arrow data. Development dependencies, pinned in
[ui/package.json](../ui/package.json), are Vite with its React plugin, Tailwind
CSS with its Vite plugin, React's type declarations, TypeScript, the oxlint linter
with its type-aware companion `oxlint-tsgolint`, the oxfmt formatter, and
Node's type declarations (`@types/node`) for the Vite configuration.

The Vite configuration is TypeScript that runs in Node. It is type-checked
against `@types/node` through [ui/tsconfig.node.json](../ui/tsconfig.node.json)
and linted with the Node rules in [ui/.oxlintrc.json](../ui/.oxlintrc.json).
