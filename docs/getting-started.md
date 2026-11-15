# Getting Started

This guide prepares a dageist checkout and opens the application. A successful
run bundles the React view, compiles the Tauri host, opens the native window and
shows the identity the Rust core reports.

## Prerequisites

- Git.
- Rust installed through rustup. [rust-toolchain.toml](../rust-toolchain.toml)
  selects the nightly toolchain the Polars release in `vendor/polars` is built
  with, `nightly-2026-09-01`, with rustfmt and Clippy; rustup installs it the
  first time a Cargo command runs in the checkout.
- A C compiler and `make`, which build the jemalloc allocator Polars uses on
  macOS and Linux.
- Node.js 20.19 or newer on the 20 line, or 22.12 or newer, with npm. The
  linter and formatter in [ui/package.json](../ui/package.json) set this floor.
- The platform's Tauri 2 system dependencies: on macOS, the Xcode Command Line
  Tools; on Linux and Windows, the WebView and build packages listed in the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

Toolchain installation and initial dependency retrieval are environment setup.
The checks in [How-to](how-to.md) then run offline and do not retrieve missing
packages.

## Prepare The Checkout

1. Clone the repository, then fetch its submodule, the Polars source in
   `vendor/polars`:

   ```sh
   git clone https://github.com/bj-data-eng/dageist.git
   cd dageist
   git submodule update --init --depth 1
   ```

   The last command fetches about 13 MB: the Polars commit the repository is
   pinned to, and the newest commit of Polars' main branch, which Git fetches
   first. It takes no history and no other branches or tags. The checked-out
   files take about 27 MB more. The same command fills `vendor/polars` in an
   existing clone. Without `--depth 1`, Git fetches the pinned commit's
   whole history, about 190 MB.
   `git clone --recurse-submodules` instead fetches the newest commit of
   every Polars branch and tag, about 150 MB, and grows as Polars adds tags.

2. Retrieve the locked Rust dependencies into the local Cargo cache. Besides
   crates.io, this fetches the two crates Polars patches from their git
   repositories:

   ```sh
   cargo fetch --locked
   ```

3. Install the view's locked dependencies into `ui/node_modules/`:

   ```sh
   npm --prefix ui ci
   ```

## Open The Application

1. Compile the view into `ui/dist/`. The host embeds this directory, so build it
   before compiling or running `dageist-tauri`:

   ```sh
   npm --prefix ui run build
   ```

2. Build and open the application:

   ```sh
   cargo run --offline --locked -p dageist-tauri
   ```

The first build compiles Tauri, Polars and their dependencies and takes several
minutes. A dark window titled `dageist` then opens on a transform: the rows
after the chosen step in the centre and the steps listed on the right. Choosing
a step shows its rows, and `Open the editor` opens the steps in a second
window, where they are edited. There is a `Table` workspace beside it that
[generates its sample](how-to.md#open-the-sample-table) when first opened, and a
`Pieces` tab that shows a design prototype of how the editor is drawn. The
status badge changes from `Connecting to core…` to `dageist 0.1.0`, which the view
received from the Rust core through the `core_identity` IPC command. If the
badge reads `Core unavailable: …`, the view loaded but the IPC call failed.

Close the window to stop the application.

## Next Step

Read the [explanation](explanation.md) to see which layer owns which behavior,
then use [How-to](how-to.md) to run the focused checks before changing code.
