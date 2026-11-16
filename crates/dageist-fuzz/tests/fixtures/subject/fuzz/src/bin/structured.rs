#![forbid(unsafe_code)]
#![cfg_attr(feature = "coverage", no_main)]

#[cfg(not(feature = "coverage"))]
fn main() -> dageist_fuzz::Result<()> {
    dageist_fuzz::worker::serve(&dageist_fixture_fuzz::StructuredTarget::default())
}

#[cfg(feature = "coverage")]
static TARGET: std::sync::OnceLock<dageist_fixture_fuzz::StructuredTarget> =
    std::sync::OnceLock::new();

#[cfg(feature = "coverage")]
std::thread_local! {
    static DRIVER: std::cell::RefCell<dageist_fuzz::engine::CoverageDriver<'static, dageist_fixture_fuzz::StructuredTarget>> =
        std::cell::RefCell::new(dageist_fuzz::engine::CoverageDriver::for_campaign(TARGET.get_or_init(dageist_fixture_fuzz::StructuredTarget::default), None).expect("fixture campaign configuration"));
}

#[cfg(feature = "coverage")]
libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    DRIVER.with(|driver| {
        driver
            .borrow_mut()
            .libfuzzer_entry_framed(input, &mut std::io::stdout().lock(), 1_048_576)
            .expect("fixture coverage evaluation")
    });
});
