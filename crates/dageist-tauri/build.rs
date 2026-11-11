//! Generates the Tauri context and capability schemas for the desktop host.

#![forbid(unsafe_code)]

fn main() {
    tauri_build::build();
}
