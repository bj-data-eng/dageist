//! Runs a test body in a process of its own.
//!
//! Polars keeps its set-up in the process: what is registered, what is read from
//! the environment and what is started stays so until the process ends. A test
//! that asks whether the set-up has happened, or sets an environment variable
//! for it to read, needs a process no other test has touched. The test starts
//! its own test binary again, to run only itself, with the variables it wants.

use std::process::Command;

/// The variable that tells a started process which test body to run.
const BODY: &str = "DAGEIST_TEST_BODY";

/// Runs `body` in a process of its own, with `variables` set in it.
///
/// `test` is the name of the calling test function. `prepare` runs only in the
/// calling process, and returns the variables to set; `body` runs only in the
/// process it starts, and reads its variables from the environment.
///
/// # Panics
///
/// When the process cannot be started, does not run exactly the test named, or
/// the body fails in it; the panic message holds what the process printed.
pub fn in_fresh_process(
    test: &str,
    prepare: impl FnOnce() -> Vec<(&'static str, String)>,
    body: impl FnOnce(),
) {
    if std::env::var(BODY).is_ok_and(|named| named == test) {
        body();
        return;
    }
    let variables = prepare();
    let program = std::env::current_exe().expect("the test program's own path is known");
    let output = Command::new(program)
        .args(["--exact", test, "--nocapture", "--test-threads", "1"])
        .env(BODY, test)
        .envs(variables)
        .output()
        .expect("the test program starts again");
    let printed = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
        "the process did not run exactly the test {test}\n{printed}"
    );
    assert!(
        output.status.success(),
        "the body of {test} failed in a process of its own\n{printed}"
    );
}
