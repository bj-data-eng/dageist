#![forbid(unsafe_code)]

fn main() {
    std::process::exit(dageist_fuzz::campaign::main_entry(
        std::env::args_os().skip(1),
    ));
}
