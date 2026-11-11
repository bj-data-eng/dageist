# How-To

These procedures assume [getting started](getting-started.md) succeeded. Run
commands from the repository root. [AGENTS.md](../AGENTS.md#command-inventory)
owns command selection, concurrency and host-monitoring policy.

## Check The Core

The core has no host or view dependency, so it builds and tests quickly:

```sh
cargo test --offline --locked -j 1 -p dageist --lib
cargo clippy --offline --locked -j 1 -p dageist --all-targets --no-deps -- -F unsafe-code -D warnings
```

The tests pass and Clippy reports no warnings. `-F unsafe-code` rejects any
`unsafe` code in the checked package.

## Check The View

Type-check the TypeScript sources, then compile them into `ui/dist/`:

```sh
npm --prefix ui run check
npm --prefix ui run build
```

Both commands exit successfully. `ui/dist/` then holds `index.html`,
`styles.css` and the compiled `main.js`. To restore `ui/node_modules/` from the
local npm cache without network access, run `npm --prefix ui ci --offline`.

## Check The Host

The host embeds `ui/dist/` at compile time, so [check the view](#check-the-view)
first:

```sh
cargo test --offline --locked -j 1 -p dageist-tauri
cargo clippy --offline --locked -j 1 -p dageist-tauri --all-targets --no-deps -- -F unsafe-code -D warnings
```

The tests prove the `core_identity` command returns the core's identity and
serializes it in the shape the view expects. They do not open a window.

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
