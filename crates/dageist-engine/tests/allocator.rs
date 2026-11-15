//! The engine makes Polars' own allocator the global allocator, as the
//! Python Polars release does. Polars reads the memory it has counted to
//! decide when its streaming engine spills to disk.

// A program gets the engine's global allocator by linking the engine, as the host does. A
// crate that is never named is not linked, so this test names it.
extern crate dageist_engine;

/// How much memory the test sets aside.
const BLOCK: usize = 64 << 20;

#[test]
fn memory_a_program_allocates_is_counted_by_polars() {
    let before = polars_ooc::estimate_memory_usage();

    let block = std::hint::black_box(vec![1_u8; BLOCK]);
    let during = polars_ooc::estimate_memory_usage();
    drop(block);

    // Other threads allocate too, so only the set-aside block is required to show.
    assert!(
        during >= before + BLOCK as u64 / 2,
        "Polars counted {during} bytes with a {BLOCK}-byte block set aside, against {before} before it",
    );
}
