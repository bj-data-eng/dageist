# Reference

## Packages And Workspace

The root [Cargo.toml](../Cargo.toml) declares `dageist` 0.1.0, Rust edition
2024, MSRV 1.99.1 and the MIT license. It forbids `unsafe` code for every
workspace member through `[workspace.lints.rust]`. The workspace uses
`default-members = ["."]` and one committed root [Cargo.lock](../Cargo.lock).

| Package | Path | Role |
| --- | --- | --- |
| `dageist` | [src/](../src/lib.rs) | Host-independent core library. |
| `dageist-tauri` | [crates/dageist-tauri/](../crates/dageist-tauri/Cargo.toml) | Tauri 2 host; builds the `dageist` binary. Not published. |
| `dageist-ui` | [ui/](../ui/package.json) | Private npm package for the TypeScript view. |

## Core API

| Item | Returns |
| --- | --- |
| `dageist::crate_name()` | The core's package name, `"dageist"`. |
| `dageist::crate_version()` | The core's package version. |

## IPC Commands

The host registers these commands in
[crates/dageist-tauri/src/main.rs](../crates/dageist-tauri/src/main.rs). The view
calls them through `window.__TAURI__.core.invoke`.

| Command | Arguments | Result |
| --- | --- | --- |
| `core_identity` | None | `{ "name": string, "version": string }` from the core. |

## Host Configuration

[crates/dageist-tauri/tauri.conf.json](../crates/dageist-tauri/tauri.conf.json)
sets the following:

| Setting | Value |
| --- | --- |
| Bundle identifier | `io.github.bj-data-eng.dageist` |
| View | `../../ui/dist`, embedded at compile time; build it with `npm --prefix ui run build` first |
| Global API | `app.withGlobalTauri` is `true`, which injects `window.__TAURI__` |
| Main window | Label `main`, title `dageist`, 1440×900, minimum 960×600, dark theme |
| Content security policy | Same-origin scripts, styles and images, plus `data:` images and the IPC channel |

[crates/dageist-tauri/capabilities/default.json](../crates/dageist-tauri/capabilities/default.json)
grants the `main` window the `core:default` permission set.

## View Files

| Path | Contents |
| --- | --- |
| [ui/src/main.ts](../ui/src/main.ts) | Shell rendering and IPC calls. |
| [ui/src/tauri.d.ts](../ui/src/tauri.d.ts) | Types for the injected `window.__TAURI__` members the view uses. |
| [ui/public/](../ui/public/index.html) | `index.html` and `styles.css`, copied into `ui/dist/`. |
| [ui/scripts/copy-public.mjs](../ui/scripts/copy-public.mjs) | Build step that copies `ui/public/` into `ui/dist/`. |
| [ui/tsconfig.json](../ui/tsconfig.json) | Strict compiler settings; emits ES2022 modules into `ui/dist/`. |
| [ui/.oxlintrc.json](../ui/.oxlintrc.json) | Lint rules: the `correctness` and `suspicious` categories plus the listed rules, all as errors, with type-aware rules enabled. |
| [ui/.oxfmtrc.json](../ui/.oxfmtrc.json) | Formatter settings: 100-column lines, single quotes, semicolons. |

| npm script | Action |
| --- | --- |
| `check` | Type-checks the view without emitting files. |
| `lint` | Lints `ui/` with oxlint, denying warnings and reporting unused suppressions. |
| `format` | Rewrites the files in `ui/` with oxfmt. |
| `format:check` | Reports files that oxfmt would change, without writing. |
| `build` | Compiles the view into `ui/dist/` and copies `ui/public/` beside it. |

The view has no runtime npm dependencies and no bundler. Its development
dependencies, pinned to exact versions in [ui/package.json](../ui/package.json),
are TypeScript, the oxlint linter with its type-aware companion
`oxlint-tsgolint`, and the oxfmt formatter.

The lint configuration has one override: build scripts under `ui/scripts/` are
plain JavaScript that uses Node APIs, and Node's type declarations are not
installed, so the two rules that would report every Node call as untyped are
off for those files.
