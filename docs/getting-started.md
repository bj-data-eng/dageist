# Getting Started

This guide prepares a dageist checkout and opens the application. A successful
run compiles the TypeScript view and the Tauri host, opens the native window and
shows the identity the Rust core reports.

## Prerequisites

- Git.
- Rust 1.99.1 or newer with Cargo, matching the compatibility floor in
  [Cargo.toml](../Cargo.toml).
- Node.js 20.19 or newer on the 20 line, or 22.12 or newer, with npm. The
  linter and formatter in [ui/package.json](../ui/package.json) set this floor.
- The platform's Tauri 2 system dependencies: on macOS, the Xcode Command Line
  Tools; on Linux and Windows, the WebView and build packages listed in the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

Toolchain installation and initial dependency retrieval are environment setup.
The checks in [How-to](how-to.md) then run offline and do not retrieve missing
packages.

## Prepare The Checkout

1. Clone the repository:

   ```sh
   git clone https://github.com/bj-data-eng/dageist.git
   cd dageist
   ```

2. Retrieve the locked Rust dependencies into the local Cargo cache:

   ```sh
   cargo fetch --locked
   ```

3. Install the view's locked development dependencies into `ui/node_modules/`:

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
minutes.
A dark window titled `dageist` then opens on an empty dotted canvas. Its status
badge changes from `Connecting to core…` to `dageist 0.1.0`, which the view
received from the Rust core through the `core_identity` IPC command. If the
badge reads `Core unavailable: …`, the view loaded but the IPC call failed.

Close the window to stop the application.

## Next Step

Read the [explanation](explanation.md) to see which layer owns which behavior,
then use [How-to](how-to.md) to run the focused checks before changing code.
