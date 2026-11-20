//! The methods of text built through the catalogue and run on small tables, against the values
//! Polars' documentation gives them, which are written here by hand, and the dtypes it documents:
//! a length is a `UInt32`, a test is a `Boolean`, a split is a list of text and a join is one
//! text.

use polars::prelude::{DataFrame, DataType, IntoLazy, col, df, lit};

use crate::catalog::{Argument, Arguments, BuildError};

use super::build::{dtype_of, expression_arg, flag, method, none, run, text};

/// The text `id` gives each row of `frame`'s column `t`, a null as `None`.
fn texts(frame: &DataFrame, id: &str, arguments: &Arguments) -> Vec<Option<String>> {
    let expr = method(id, col("t"), arguments).expect("the method builds");
    let out = run(frame.clone().lazy().select([expr.alias("out")]));
    out.column("out")
        .expect("a column")
        .str()
        .expect("text")
        .iter()
        .map(|value| value.map(str::to_owned))
        .collect()
}

fn some(values: &[Option<&str>]) -> Vec<Option<String>> {
    values
        .iter()
        .map(|value| value.map(str::to_owned))
        .collect()
}

fn whole(value: i64) -> Argument {
    expression_arg(lit(value))
}

fn given(name: &'static str, argument: Argument) -> Arguments {
    Arguments::new().with(name, argument)
}

#[test]
fn the_case_of_text_changes_as_the_documentation_shows() {
    let quotes = df!("t" => [
        Some("'e.t. phone home'"),
        Some("you talkin' to me?"),
        Some("to infinity,and BEYOND!"),
        None,
    ])
    .unwrap();
    assert_eq!(
        texts(&quotes, "Expr.str.to_titlecase", &none()),
        some(&[
            Some("'E.T. Phone Home'"),
            Some("You Talkin' To Me?"),
            Some("To Infinity,And Beyond!"),
            None,
        ])
    );
    let mixed = df!("t" => [Some("Café Ünï"), None]).unwrap();
    assert_eq!(
        texts(&mixed, "Expr.str.to_uppercase", &none()),
        some(&[Some("CAFÉ ÜNÏ"), None])
    );
    assert_eq!(
        texts(&mixed, "Expr.str.to_lowercase", &none()),
        some(&[Some("café ünï"), None])
    );
}

#[test]
fn text_is_reversed_by_characters_and_not_by_bytes() {
    let words = df!("t" => [Some("héllo"), Some(""), None]).unwrap();
    assert_eq!(
        texts(&words, "Expr.str.reverse", &none()),
        some(&[Some("olléh"), Some(""), None])
    );
}

#[test]
fn lengths_count_characters_and_bytes_and_are_unsigned_counts() {
    // `Café` is four characters and five bytes, `東京` two characters and six bytes.
    let words = df!("t" => [Some("Café"), Some("345"), Some("東京"), None]).unwrap();
    let counted = |id: &str| -> Vec<Option<u32>> {
        let expr = method(id, col("t"), &none()).expect("the method builds");
        run(words.clone().lazy().select([expr.alias("out")]))
            .column("out")
            .expect("a column")
            .u32()
            .expect("unsigned counts")
            .iter()
            .collect()
    };
    assert_eq!(
        counted("Expr.str.len_chars"),
        [Some(4), Some(3), Some(2), None]
    );
    assert_eq!(
        counted("Expr.str.len_bytes"),
        [Some(5), Some(3), Some(6), None]
    );
}

#[test]
fn trimming_takes_whitespace_when_no_characters_are_given_and_those_given_otherwise() {
    let words = df!("t" => [" hello", "\nworld"]).unwrap();
    assert_eq!(
        texts(&words, "Expr.str.strip_chars", &none()),
        some(&[Some("hello"), Some("world")])
    );
    // A string given strips the characters in it from both ends, and whitespace only if it
    // is among them.
    let characters = || given("characters", expression_arg(lit("ow\n")));
    assert_eq!(
        texts(&words, "Expr.str.strip_chars", &characters()),
        some(&[Some(" hell"), Some("rld")])
    );
    // One side only.
    let ends = df!("t" => ["xxhixx"]).unwrap();
    let x = || given("characters", expression_arg(lit("x")));
    assert_eq!(
        texts(&ends, "Expr.str.strip_chars_start", &x()),
        some(&[Some("hixx")])
    );
    assert_eq!(
        texts(&ends, "Expr.str.strip_chars_end", &x()),
        some(&[Some("xxhi")])
    );
    assert_eq!(
        texts(&ends, "Expr.str.strip_chars", &x()),
        some(&[Some("hi")])
    );
}

#[test]
fn a_prefix_or_a_suffix_is_taken_off_once_and_only_where_it_is_there() {
    let prefixed = df!("t" => ["foobar", "foofoobar", "foo", "bar"]).unwrap();
    assert_eq!(
        texts(
            &prefixed,
            "Expr.str.strip_prefix",
            &given("prefix", expression_arg(lit("foo")))
        ),
        some(&[Some("bar"), Some("foobar"), Some(""), Some("bar")])
    );
    let suffixed = df!("t" => ["foobar", "foobarbar", "foo", "bar"]).unwrap();
    assert_eq!(
        texts(
            &suffixed,
            "Expr.str.strip_suffix",
            &given("suffix", expression_arg(lit("bar")))
        ),
        some(&[Some("foo"), Some("foobar"), Some("foo"), Some("")])
    );
}

#[test]
fn a_test_for_a_start_or_an_end_is_a_boolean_with_a_null_for_a_null() {
    let words = df!("t" => [Some("foobar"), Some("bar"), None]).unwrap();
    let tested = |id: &str, parameter: &'static str, what: &str| -> Vec<Option<bool>> {
        let expr = method(
            id,
            col("t"),
            &given(parameter, expression_arg(lit(what.to_owned()))),
        )
        .expect("the method builds");
        run(words.clone().lazy().select([expr.alias("out")]))
            .column("out")
            .expect("a column")
            .bool()
            .expect("booleans")
            .iter()
            .collect()
    };
    assert_eq!(
        tested("Expr.str.starts_with", "prefix", "foo"),
        [Some(true), Some(false), None]
    );
    assert_eq!(
        tested("Expr.str.ends_with", "suffix", "bar"),
        [Some(true), Some(true), None]
    );
    assert_eq!(
        dtype_of(
            &method(
                "Expr.str.starts_with",
                col("s"),
                &given("prefix", expression_arg(lit("a")))
            )
            .unwrap()
        ),
        DataType::Boolean
    );
}

#[test]
fn a_piece_is_taken_from_a_place_the_head_or_the_tail_as_the_documentation_shows() {
    let fruit = df!("t" => [Some("pear"), None, Some("papaya"), Some("dragonfruit")]).unwrap();
    // A negative offset counts from the end.
    assert_eq!(
        texts(&fruit, "Expr.str.slice", &given("offset", whole(-3))),
        some(&[Some("ear"), None, Some("aya"), Some("uit")])
    );
    // Up to five characters from the start, and from the end.
    assert_eq!(
        texts(&fruit, "Expr.str.head", &given("n", whole(5))),
        some(&[Some("pear"), None, Some("papay"), Some("drago")])
    );
    assert_eq!(
        texts(&fruit, "Expr.str.tail", &given("n", whole(5))),
        some(&[Some("pear"), None, Some("apaya"), Some("fruit")])
    );
    // A length ends the slice, from the offset.
    let slice = Arguments::new()
        .with("offset", whole(1))
        .with("length", whole(3));
    assert_eq!(
        texts(&fruit, "Expr.str.slice", &slice),
        some(&[Some("ear"), None, Some("apa"), Some("rag")])
    );
    // A negative count leaves out that many from the other end.
    assert_eq!(
        texts(&fruit, "Expr.str.head", &given("n", whole(-2))),
        some(&[Some("pe"), None, Some("papa"), Some("dragonfru")])
    );
}

#[test]
fn a_slice_counts_characters_and_a_count_may_be_a_column() {
    let words = df!("t" => ["héllo"], "n" => [2_i64]).unwrap();
    assert_eq!(
        texts(&words, "Expr.str.slice", &given("offset", whole(1))),
        some(&[Some("éllo")])
    );
    assert_eq!(
        texts(
            &words,
            "Expr.str.head",
            &given("n", expression_arg(col("n")))
        ),
        some(&[Some("hé")])
    );
}

#[test]
fn padding_adds_the_character_given_up_to_the_length_and_never_cuts() {
    let animals = df!("t" => [Some("cow"), Some("monkey"), Some("hippopotamus"), None]).unwrap();
    let to_eight = |character: Option<&str>| {
        let mut arguments = given("length", whole(8));
        if let Some(character) = character {
            arguments = arguments.with("fill_char", text(character));
        }
        arguments
    };
    assert_eq!(
        texts(&animals, "Expr.str.pad_start", &to_eight(Some("*"))),
        some(&[
            Some("*****cow"),
            Some("**monkey"),
            Some("hippopotamus"),
            None
        ])
    );
    assert_eq!(
        texts(&animals, "Expr.str.pad_end", &to_eight(Some("*"))),
        some(&[
            Some("cow*****"),
            Some("monkey**"),
            Some("hippopotamus"),
            None
        ])
    );
    // A space is what is added where no character is given.
    assert_eq!(
        texts(&animals, "Expr.str.pad_start", &to_eight(None))[0],
        Some("     cow".to_owned())
    );
}

#[test]
fn zeros_are_added_after_a_sign_to_the_length() {
    let numbers = df!("t" => [Some("-1"), Some("123"), Some("999999"), None]).unwrap();
    assert_eq!(
        texts(&numbers, "Expr.str.zfill", &given("length", whole(4))),
        some(&[Some("-001"), Some("0123"), Some("999999"), None])
    );
}

#[test]
fn a_fill_character_is_one_character_as_python_takes_it() {
    for wrong in ["", "ab"] {
        for id in ["Expr.str.pad_start", "Expr.str.pad_end"] {
            let refused = method(
                id,
                col("t"),
                &Arguments::new()
                    .with("length", whole(4))
                    .with("fill_char", text(wrong)),
            );
            assert!(
                matches!(
                    refused,
                    Err(BuildError::OutOfRange {
                        parameter: "fill_char",
                        ..
                    })
                ),
                "{id} with {wrong:?}"
            );
        }
    }
    // One character of more than one byte is one character.
    let fruit = df!("t" => ["pear"]).unwrap();
    assert_eq!(
        texts(
            &fruit,
            "Expr.str.pad_end",
            &Arguments::new()
                .with("length", whole(6))
                .with("fill_char", text("é"))
        ),
        some(&[Some("pearéé")])
    );
}

#[test]
fn a_split_gives_a_list_of_text_and_keeps_the_separator_only_when_asked() {
    let words = df!("t" => [Some("foo bar"), Some("foo_bar"), Some("foo_bar_baz"), None]).unwrap();
    let split = |arguments: &Arguments| pieces(&words, arguments);
    let by = || given("by", expression_arg(lit("_")));
    let list = |pieces: &[&str]| Some(pieces.iter().map(|piece| (*piece).to_owned()).collect());
    assert_eq!(
        split(&by()),
        [
            list(&["foo bar"]),
            list(&["foo", "bar"]),
            list(&["foo", "bar", "baz"]),
            None
        ]
    );
    assert_eq!(
        split(&by().with("inclusive", flag(true))),
        [
            list(&["foo bar"]),
            list(&["foo_", "bar"]),
            list(&["foo_", "bar_", "baz"]),
            None
        ]
    );
    assert_eq!(
        dtype_of(&method("Expr.str.split", col("s"), &by()).unwrap()),
        DataType::List(Box::new(DataType::String))
    );
}

/// The pieces `Expr.str.split` makes of each row of `words`, a null row as `None`.
fn pieces(words: &DataFrame, arguments: &Arguments) -> Vec<Option<Vec<String>>> {
    let expr = method("Expr.str.split", col("t"), arguments).expect("the method builds");
    let out = run(words.clone().lazy().select([expr.alias("out")]));
    let lists = out
        .column("out")
        .expect("a column")
        .list()
        .expect("lists")
        .clone();
    (0..lists.len())
        .map(|at| {
            lists.get_as_series(at).map(|row| {
                row.str()
                    .expect("text")
                    .iter()
                    .map(|piece| piece.expect("a piece").to_owned())
                    .collect()
            })
        })
        .collect()
}

#[test]
fn a_split_by_a_pattern_splits_where_the_pattern_matches_as_the_documentation_shows() {
    // The documentation splits at a run of digits, and keeps what it matched with the piece
    // before it where it is asked to.
    let words = df!("t" => [Some("foo1bar"), Some("foo99bar"), Some("foo1bar2baz"), None]).unwrap();
    let by = || given("by", expression_arg(lit(r"\d+"))).with("literal", flag(false));
    let list = |pieces: &[&str]| Some(pieces.iter().map(|piece| (*piece).to_owned()).collect());
    assert_eq!(
        pieces(&words, &by()),
        [
            list(&["foo", "bar"]),
            list(&["foo", "bar"]),
            list(&["foo", "bar", "baz"]),
            None
        ]
    );
    assert_eq!(
        pieces(&words, &by().with("inclusive", flag(true))),
        [
            list(&["foo1", "bar"]),
            list(&["foo99", "bar"]),
            list(&["foo1", "bar2", "baz"]),
            None
        ]
    );
    // A pattern gives the same dtype as a literal separator does.
    assert_eq!(
        dtype_of(&method("Expr.str.split", col("s"), &by()).unwrap()),
        DataType::List(Box::new(DataType::String))
    );
}

#[test]
fn a_split_treats_its_separator_as_plain_text_when_literal_and_ignores_strict_then() {
    // The same separator is a pattern or plain text by the flag: `.` is any character as a
    // pattern and only a dot as text.
    let words = df!("t" => ["a.b"]).unwrap();
    let by = || given("by", expression_arg(lit(".")));
    let list = |pieces: &[&str]| Some(pieces.iter().map(|piece| (*piece).to_owned()).collect());
    assert_eq!(
        pieces(&words, &by().with("literal", flag(true))),
        [list(&["a", "b"])]
    );
    assert_eq!(
        pieces(&words, &by().with("literal", flag(false))),
        [list(&["", "", "", ""])]
    );
    // `strict` is for a pattern, so beside a literal split it changes nothing, as in Python.
    assert_eq!(
        pieces(&words, &by().with("strict", flag(false))),
        [list(&["a", "b"])]
    );
}

#[test]
fn a_split_by_a_pattern_that_is_not_one_is_refused_when_strict_and_gives_null_when_not() {
    let words = df!("t" => ["a(b"]).unwrap();
    let broken = |strict: bool| {
        given("by", expression_arg(lit("(")))
            .with("literal", flag(false))
            .with("strict", flag(strict))
    };
    let expr = method("Expr.str.split", col("t"), &broken(true)).expect("the method builds");
    let refused = words.clone().lazy().select([expr]).collect();
    assert!(
        refused
            .expect_err("a strict split refuses a pattern that is no regex")
            .to_string()
            .contains("regex"),
    );
    assert_eq!(pieces(&words, &broken(false)), [None]);
}

#[test]
fn a_join_gives_one_text_and_skips_nulls_unless_it_is_asked_to_keep_them() {
    // The values are one whole number and a null and another, joined by a dash.
    let numbers = df!("t" => [Some(1_i64), None, Some(3)]).unwrap();
    let joined = |arguments: &Arguments| -> Vec<Option<String>> {
        let expr = method("Expr.str.join", col("t"), arguments).expect("the method builds");
        run(numbers.clone().lazy().select([expr.alias("out")]))
            .column("out")
            .expect("a column")
            .str()
            .expect("text")
            .iter()
            .map(|value| value.map(str::to_owned))
            .collect()
    };
    assert_eq!(joined(&given("delimiter", text("-"))), some(&[Some("1-3")]));
    assert_eq!(joined(&none()), some(&[Some("13")]));
    assert_eq!(joined(&given("ignore_nulls", flag(false))), some(&[None]));
    // One value, whatever the rows, and text, whatever it joins.
    let one = method("Expr.str.join", col("i"), &none()).unwrap();
    assert_eq!(dtype_of(&one), DataType::String);
}

#[test]
fn a_method_of_text_refuses_whole_numbers() {
    for id in [
        "Expr.str.to_uppercase",
        "Expr.str.len_chars",
        "Expr.str.reverse",
    ] {
        let on_numbers = method(id, col("i"), &none()).expect("the method builds");
        let frame = super::build::sample().select([on_numbers.alias("out")]);
        assert!(
            frame.clone().collect().is_err(),
            "{id} refuses whole numbers"
        );
    }
}
