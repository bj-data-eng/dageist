//! The methods of text built through the catalogue and run on small tables, against the values
//! Polars' documentation gives them, which are written here by hand, and the dtypes it documents:
//! a length is a `UInt32`, a test is a `Boolean`, a split is a list of text and a join is one
//! text.

use polars::prelude::{DataFrame, DataType, Field, IntoLazy, col, df, lit};

use crate::catalog::{Argument, Arguments, BuildError, check_pattern, groups_dtype};

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

#[test]
fn a_pattern_is_asked_of_polars_and_refused_in_its_words_where_it_is_no_regular_expression() {
    // Each of these is a regular expression Rust's `regex` crate refuses, for the reason Polars
    // gives in its words, which names the place in the pattern.
    for (pattern, reason) in [
        ("(", "unclosed group"),
        ("[a", "unclosed character class"),
        ("a{2,1}", "invalid repetition count range"),
        ("(?<n>a)(?<n>d)", "duplicate capture group name"),
        ("(?z)a", "unrecognized flag"),
        (r"\p{Nothing}", "Unicode property not found"),
        ("\\", "incomplete escape sequence"),
        ("*a", "repetition operator missing expression"),
        ("a)", "unopened group"),
        ("(?:a{1000}){1000}", "exceeds size limit"),
    ] {
        let error = check_pattern(pattern).expect_err(pattern);
        assert!(error.reason().contains(reason), "{pattern}: {error}");
        // Polars refuses the same pattern when it runs a method that compiles it.
        let rows = df!("t" => ["a"]).unwrap();
        let polars = rows
            .lazy()
            .select([col("t").str().contains(lit(pattern), true)])
            .collect()
            .expect_err(pattern);
        assert!(polars.to_string().contains(reason), "{pattern}: {polars}");
        // The catalogue's build refuses it too, at the parameter, and not before it is built.
        let given = given("pattern", Argument::Pattern(pattern.to_owned()));
        let Err(BuildError::NotAPattern { parameter, .. }) =
            method("Expr.str.contains", col("t"), &given)
        else {
            panic!("{pattern} is refused");
        };
        assert_eq!(parameter, "pattern");
    }
    // Regular expressions Polars compiles, among them an empty one and one that is plain text.
    for pattern in [
        "a", "(a)", "(?<g>a)", "[a-z]+", r"\d+", "^1", "(?i)D", "", "x*",
    ] {
        assert_eq!(check_pattern(pattern), Ok(()), "{pattern}");
    }
}

#[test]
fn a_pattern_that_a_flag_makes_plain_text_is_never_refused_for_being_no_regular_expression() {
    // `(` is no regular expression, and as plain text it is found where the text holds it.
    let words = df!("t" => [Some("f(x)"), Some("fx"), None]).unwrap();
    let contains = |literal: bool| {
        let given =
            given("pattern", Argument::Pattern("(".to_owned())).with("literal", flag(literal));
        method("Expr.str.contains", col("t"), &given)
    };
    assert!(matches!(
        contains(false),
        Err(BuildError::NotAPattern {
            parameter: "pattern",
            ..
        })
    ));
    let found = run(words.lazy().select([contains(true).unwrap().alias("out")]));
    let found: Vec<Option<bool>> = found
        .column("out")
        .expect("a column")
        .bool()
        .expect("booleans")
        .iter()
        .collect();
    assert_eq!(found, [Some(true), Some(false), None]);
    // A method that has no flag takes its pattern as a regular expression, always.
    let extract = given("pattern", Argument::Pattern("(".to_owned()));
    assert!(matches!(
        method("Expr.str.extract", col("t"), &extract),
        Err(BuildError::NotAPattern { .. })
    ));
}

#[test]
fn the_fields_a_pattern_names_are_the_struct_polars_works_out_of_its_groups() {
    // A group has the name it is given, and the number it has where it has none: Polars names a
    // field of the struct that `extract_groups` gives so, and every one is text.
    let field = |name: &str| Field::new(name.into(), DataType::String);
    for (pattern, fields) in [
        ("(?<year>\\d{4})-(\\d{2})", vec!["year", "2"]),
        ("(a)(b)(c)", vec!["1", "2", "3"]),
        ("(?<g>a)", vec!["g"]),
        ("(?:a)b", vec![]),
        ("", vec![]),
    ] {
        let expected = DataType::Struct(fields.into_iter().map(field).collect());
        assert_eq!(groups_dtype(pattern).expect(pattern), expected, "{pattern}");
        // What a step gives is the dtype the method has once it is built.
        let given = given("pattern", Argument::Pattern(pattern.to_owned()));
        let built = method("Expr.str.extract_groups", col("s"), &given).expect("it builds");
        assert_eq!(dtype_of(&built), expected, "{pattern}");
    }
    assert!(groups_dtype("(").is_err());
}

// ---------------------------------------------------------------------------
// The methods that take a pattern, against what Polars' documentation shows and what Python
// Polars 2.1 gives: the expected cells are written here by hand.

fn pattern(text: &str) -> Argument {
    Argument::Pattern(text.to_owned())
}

/// The rows the methods of patterns are run on: a null, a text with a dollar sign, two dates.
fn dated() -> DataFrame {
    df!("t" => [
        Some("Crab"),
        Some("cat and dog"),
        Some("rab$bit"),
        None,
        Some("2024-03-10 x 2025-04-11"),
    ])
    .unwrap()
}

fn column_of<T>(
    frame: &DataFrame,
    id: &str,
    arguments: &Arguments,
    read: impl Fn(&polars::prelude::Column) -> Vec<Option<T>>,
) -> Vec<Option<T>> {
    let expr = method(id, col("t"), arguments).expect("the method builds");
    let out = run(frame.clone().lazy().select([expr.alias("out")]));
    read(out.column("out").expect("a column"))
}

fn booleans(frame: &DataFrame, id: &str, arguments: &Arguments) -> Vec<Option<bool>> {
    column_of(frame, id, arguments, |out| {
        out.bool().expect("booleans").iter().collect()
    })
}

fn counts(frame: &DataFrame, id: &str, arguments: &Arguments) -> Vec<Option<u32>> {
    column_of(frame, id, arguments, |out| {
        out.u32().expect("counts").iter().collect()
    })
}

#[test]
fn a_test_for_a_pattern_finds_a_regular_expression_or_plain_text_as_the_flag_says() {
    let rows = dated();
    let given = |text: &str| given("pattern", pattern(text));
    assert_eq!(
        booleans(&rows, "Expr.str.contains", &given("cat|bit")),
        [Some(false), Some(true), Some(true), None, Some(false)]
    );
    // `$` is the end of the text as a pattern, and a dollar sign as plain text.
    assert_eq!(
        booleans(&rows, "Expr.str.contains", &given("rab$")),
        [Some(true), Some(false), Some(false), None, Some(false)]
    );
    assert_eq!(
        booleans(
            &rows,
            "Expr.str.contains",
            &given("rab$").with("literal", flag(true))
        ),
        [Some(false), Some(false), Some(true), None, Some(false)]
    );
    // A flag of the pattern, as the syntax of Rust's `regex` has them.
    assert_eq!(
        booleans(&rows, "Expr.str.contains", &given("(?i)CRAB")),
        [Some(true), Some(false), Some(false), None, Some(false)]
    );
    // A test gives a `Boolean`, a null for a null.
    assert_eq!(
        dtype_of(&method("Expr.str.contains", col("s"), &given("a")).unwrap()),
        DataType::Boolean
    );
}

#[test]
fn matches_are_counted_and_found_by_their_place_in_bytes() {
    let rows = dated();
    let digits = || given("pattern", pattern(r"\d"));
    assert_eq!(
        counts(&rows, "Expr.str.count_matches", &digits()),
        [Some(0), Some(0), Some(0), None, Some(16)]
    );
    let a = || given("pattern", pattern("a"));
    assert_eq!(
        counts(
            &rows,
            "Expr.str.count_matches",
            &a().with("literal", flag(true))
        ),
        [Some(1), Some(2), Some(1), None, Some(0)]
    );
    // The place is where the first match starts, counted in bytes from 0, and nothing where
    // there is none.
    assert_eq!(
        counts(&rows, "Expr.str.find", &a()),
        [Some(2), Some(1), Some(1), None, None]
    );
    let dollar = || given("pattern", pattern("$"));
    assert_eq!(
        counts(
            &rows,
            "Expr.str.find",
            &dollar().with("literal", flag(true))
        ),
        [None, None, Some(3), None, None]
    );
    // `é` takes two bytes, so what follows it is two places further than its characters say.
    let accented = df!("t" => ["aé b"]).unwrap();
    assert_eq!(
        counts(&accented, "Expr.str.find", &given("pattern", pattern("b"))),
        [Some(4)]
    );
    for id in ["Expr.str.count_matches", "Expr.str.find"] {
        assert_eq!(
            dtype_of(&method(id, col("s"), &a()).unwrap()),
            DataType::UInt32,
            "{id}"
        );
    }
}

#[test]
fn a_replacement_puts_text_for_the_first_match_or_for_each_and_refers_to_groups_as_the_regex_crate_does()
 {
    let rows = dated();
    let replacing = |text: &str, with: &str| {
        given("pattern", pattern(text)).with("value", expression_arg(lit(with)))
    };
    assert_eq!(
        texts(&rows, "Expr.str.replace", &replacing("a", "_")),
        some(&[
            Some("Cr_b"),
            Some("c_t and dog"),
            Some("r_b$bit"),
            None,
            Some("2024-03-10 x 2025-04-11")
        ])
    );
    assert_eq!(
        texts(&rows, "Expr.str.replace_all", &replacing("a", "_")),
        some(&[
            Some("Cr_b"),
            Some("c_t _nd dog"),
            Some("r_b$bit"),
            None,
            Some("2024-03-10 x 2025-04-11")
        ])
    );
    // `$1` and `${name}` are the text of a group of the match, and `$$` is a dollar sign.
    assert_eq!(
        texts(
            &rows,
            "Expr.str.replace",
            &replacing(r"(\d{4})-(\d{2})-(\d{2})", "${3}/$2/$1")
        ),
        some(&[
            Some("Crab"),
            Some("cat and dog"),
            Some("rab$bit"),
            None,
            Some("10/03/2024 x 2025-04-11")
        ])
    );
    assert_eq!(
        texts(
            &rows,
            "Expr.str.replace_all",
            &replacing(r"(?<y>\d{4})-(\d{2})-(\d{2})", "${y}")
        ),
        some(&[
            Some("Crab"),
            Some("cat and dog"),
            Some("rab$bit"),
            None,
            Some("2024 x 2025")
        ])
    );
    assert_eq!(
        texts(&rows, "Expr.str.replace", &replacing("b", "$$"))
            .into_iter()
            .take(3)
            .collect::<Vec<_>>(),
        some(&[Some("Cra$"), Some("cat and dog"), Some("ra$$bit")])
    );
    // As plain text, the pattern and what replaces it mean what they say, `$1` included.
    assert_eq!(
        texts(
            &rows,
            "Expr.str.replace",
            &replacing("b", "$1").with("literal", flag(true))
        )
        .into_iter()
        .take(3)
        .collect::<Vec<_>>(),
        some(&[Some("Cra$1"), Some("cat and dog"), Some("ra$1$bit")])
    );
    // The first `n` matches of plain text: two of the three `a` of a text that holds them.
    let counted = df!("t" => ["banana"]).unwrap();
    for (n, expected) in [(0, "banana"), (1, "b_nana"), (2, "b_n_na"), (-1, "b_n_n_")] {
        let arguments = replacing("a", "_")
            .with("literal", flag(true))
            .with("n", Argument::Integer(n));
        assert_eq!(
            texts(&counted, "Expr.str.replace", &arguments),
            some(&[Some(expected)]),
            "n = {n}"
        );
    }
}

#[test]
fn a_group_is_extracted_by_its_number_and_every_match_by_the_pattern() {
    let rows = dated();
    let dates = || given("pattern", pattern(r"(\d{4})-(\d{2})"));
    // Group 1 is the default, 0 is the whole match, and a group the pattern lacks is a null.
    assert_eq!(
        texts(&rows, "Expr.str.extract", &dates()),
        some(&[None, None, None, None, Some("2024")])
    );
    for (index, expected) in [(0, "2024-03"), (2, "03")] {
        let arguments = dates().with("group_index", Argument::Integer(index));
        assert_eq!(
            texts(&rows, "Expr.str.extract", &arguments)
                .into_iter()
                .last()
                .flatten(),
            Some(expected.to_owned()),
            "group {index}"
        );
    }
    let lacking = dates().with("group_index", Argument::Integer(5));
    assert_eq!(
        texts(&rows, "Expr.str.extract", &lacking)
            .into_iter()
            .last()
            .flatten(),
        None
    );
    // A negative index is no unsigned whole number, as Python's binding refuses it.
    let negative = dates().with("group_index", Argument::Integer(-1));
    assert!(matches!(
        method("Expr.str.extract", col("t"), &negative),
        Err(BuildError::OutOfRange { .. })
    ));
    // Every match, as a list of text: none for a text that has none, and a null for a null.
    let all = column_of(
        &rows,
        "Expr.str.extract_all",
        &given("pattern", pattern(r"\d+")),
        |out| {
            let lists = out.list().expect("lists").clone();
            (0..lists.len())
                .map(|at| {
                    lists.get_as_series(at).map(|row| {
                        row.str()
                            .expect("text")
                            .iter()
                            .map(|piece| piece.expect("a match").to_owned())
                            .collect::<Vec<_>>()
                    })
                })
                .collect()
        },
    );
    assert_eq!(
        all,
        [
            Some(vec![]),
            Some(vec![]),
            Some(vec![]),
            None,
            Some(
                ["2024", "03", "10", "2025", "04", "11"]
                    .map(str::to_owned)
                    .to_vec()
            )
        ]
    );
    assert_eq!(
        dtype_of(
            &method(
                "Expr.str.extract_all",
                col("s"),
                &given("pattern", pattern("a"))
            )
            .unwrap()
        ),
        DataType::List(Box::new(DataType::String))
    );
}

#[test]
fn the_groups_of_a_pattern_are_the_fields_of_a_struct_of_what_each_matched() {
    let rows = dated();
    let arguments = given("pattern", pattern(r"(?<year>\d{4})-(\d{2})"));
    let expr = method("Expr.str.extract_groups", col("t"), &arguments).expect("it builds");
    let out = run(rows.lazy().select([expr.alias("out")]));
    let grouped = out
        .column("out")
        .expect("a column")
        .struct_()
        .expect("a struct")
        .clone();
    let cells = |name: &str| -> Vec<Option<String>> {
        grouped
            .field_by_name(name)
            .expect("a field")
            .str()
            .expect("text")
            .iter()
            .map(|cell| cell.map(str::to_owned))
            .collect()
    };
    // A text with a match fills each field; one without leaves each field a null.
    assert_eq!(
        cells("year").last().cloned().flatten(),
        Some("2024".to_owned())
    );
    assert_eq!(cells("2").last().cloned().flatten(), Some("03".to_owned()));
    assert_eq!(cells("year")[0], None);
    // A pattern with no groups is a struct with no fields, which Polars allows.
    let none = given("pattern", pattern("a"));
    assert_eq!(
        dtype_of(&method("Expr.str.extract_groups", col("s"), &none).unwrap()),
        DataType::Struct(Vec::new())
    );
}

#[test]
fn the_characters_of_a_pattern_are_escaped_to_mean_themselves() {
    let rows = dated();
    assert_eq!(
        texts(&rows, "Expr.str.escape_regex", &none()),
        some(&[
            Some("Crab"),
            Some("cat and dog"),
            Some(r"rab\$bit"),
            None,
            Some(r"2024\-03\-10 x 2025\-04\-11")
        ])
    );
}
