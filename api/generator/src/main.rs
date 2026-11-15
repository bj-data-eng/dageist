//! Generates, checks or lists the public API audits under `api/`.

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("generator lives under dageist/api/generator");

    if let Err(error) =
        dageist_api_generator::run(root, &dageist_api_generator::DAGEIST, std::env::args())
    {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
