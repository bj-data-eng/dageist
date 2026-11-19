//! A default read as a value, against Python's own literal syntax and against
//! every default the inventory writes.

use crate::catalog::{DefaultValue, Kind, ParameterDefault, Role, entries};

use super::inventory::items;

fn read(text: &str) -> DefaultValue {
    DefaultValue::from_python(text)
}

fn text(value: &str) -> DefaultValue {
    DefaultValue::Text(value.to_owned())
}

#[test]
fn python_literals_are_read_as_python_writes_them() {
    // Each case is Python's syntax for the value on the right, from the language reference.
    let cases = [
        ("None", DefaultValue::Unset),
        ("True", DefaultValue::Flag(true)),
        ("False", DefaultValue::Flag(false)),
        ("()", DefaultValue::Empty),
        ("{}", DefaultValue::Empty),
        ("0", DefaultValue::Integer(0)),
        ("1", DefaultValue::Integer(1)),
        ("-1", DefaultValue::Integer(-1)),
        ("+7", DefaultValue::Integer(7)),
        ("1_000", DefaultValue::Integer(1000)),
        ("0.0", DefaultValue::Float(0.0)),
        ("0.001", DefaultValue::Float(0.001)),
        ("1e-09", DefaultValue::Float(1e-9)),
        ("2E3", DefaultValue::Float(2000.0)),
        ("-1.5", DefaultValue::Float(-1.5)),
        (".5", DefaultValue::Float(0.5)),
        ("1.", DefaultValue::Float(1.0)),
        ("1_0.2_5", DefaultValue::Float(10.25)),
        ("'auto'", text("auto")),
        ("\"auto\"", text("auto")),
        ("''", text("")),
        ("' '", text(" ")),
        ("'{}'", text("{}")),
        ("'m:m'", text("m:m")),
        // Python writes a text holding a double quote in single quotes, and the reverse.
        ("'\"'", text("\"")),
        ("\"it's\"", text("it's")),
        ("'it\\'s'", text("it's")),
        // Escapes Python writes for a new line and a backslash.
        ("'\\n'", text("\n")),
        ("'a\\\\b'", text("a\\b")),
        ("'\\t\\r'", text("\t\r")),
    ];
    for (python, expected) in cases {
        assert_eq!(read(python), expected, "{python}");
    }
}

#[test]
fn the_constant_python_names_math_e_is_read_as_the_number_it_is() {
    // `math.e` is Euler's number, the default of the base of a logarithm; Rust's constant of
    // the same number is the independent value.
    assert_eq!(read("math.e"), DefaultValue::Float(std::f64::consts::E));
}

#[test]
fn what_python_writes_otherwise_is_not_read_as_a_value() {
    let others = [
        // Names, attributes and calls.
        "DEFAULT_QUERY_OPT_FLAGS",
        "math.pi",
        "e",
        "Int64",
        "float('inf')",
        // Containers other than the empty `()` and `{}`.
        "(0.25, 0.5, 0.75)",
        "(16.0, 12.0)",
        "[]",
        "[1]",
        "{'a': 1}",
        // Numbers this reading does not take: other bases, and beyond the types held.
        "0x10",
        "0b1",
        "1__0",
        "1_",
        "1e",
        "1e+",
        "9223372036854775808",
        "1e999",
        "1j",
        // Text prefixes, joined literals, and escapes that are not read.
        "b'x'",
        "r'x'",
        "'a' 'b'",
        "'a'b'",
        "'\\x41'",
        "'\\u0041'",
        "'unfinished",
        "'",
        "\"",
        "",
        "--1",
        "- 1",
        "true",
        "none",
    ];
    for python in others {
        assert_eq!(read(python), DefaultValue::Other, "{python:?}");
    }
}

#[test]
fn the_limits_of_the_integer_type_are_read_and_beyond_them_is_not() {
    assert_eq!(read("9223372036854775807"), DefaultValue::Integer(i64::MAX));
    assert_eq!(
        read("-9223372036854775808"),
        DefaultValue::Integer(i64::MIN)
    );
    assert_eq!(read("-9223372036854775809"), DefaultValue::Other);
}

#[test]
fn a_required_parameter_has_the_default_of_none_given() {
    assert_eq!(ParameterDefault::Required.value(), DefaultValue::Required);
    assert_eq!(
        ParameterDefault::Python("1").value(),
        DefaultValue::Integer(1)
    );
}

#[test]
fn every_default_the_catalogue_writes_is_read_as_the_value_its_kind_holds() {
    for entry in entries() {
        for parameter in entry.parameters() {
            let context = format!("{} {}", entry.id().as_str(), parameter.name());
            let value = parameter.default().value();
            match (parameter.kind(), &value) {
                (_, DefaultValue::Other) => panic!("{context}: a default that is not read"),
                (_, DefaultValue::Required) => {
                    assert_eq!(parameter.default(), ParameterDefault::Required, "{context}");
                }
                (Kind::Flag, value) => assert!(
                    matches!(value, DefaultValue::Flag(_) | DefaultValue::Unset),
                    "{context}: {value:?}"
                ),
                (Kind::Choice(words), DefaultValue::Text(word)) => {
                    assert!(words.contains(&word.as_str()), "{context}: {word}");
                }
                (Kind::Choice(_), value) => {
                    assert_eq!(value, &DefaultValue::Unset, "{context}");
                }
                // A starred parameter, and `**`, hold nothing when not given.
                (Kind::Pairs(..), value) => {
                    assert_eq!(value, &DefaultValue::Empty, "{context}");
                }
                _ => {}
            }
        }
    }
}

#[test]
fn the_catalogue_defaults_read_to_the_values_of_the_python_signatures() {
    let default = |id: &str, name: &str| {
        crate::catalog::entry(id)
            .expect("an entry")
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == name)
            .expect("a parameter")
            .default()
            .value()
    };
    // `Expr.shift(n=1, *, fill_value=None)`
    assert_eq!(default("Expr.shift", "n"), DefaultValue::Integer(1));
    assert_eq!(default("Expr.shift", "fill_value"), DefaultValue::Unset);
    // `LazyFrame.join(other, on=None, how='inner', ..., suffix='_right', ..., coalesce=None,
    // ..., allow_parallel=True, force_parallel=False)`
    assert_eq!(default("LazyFrame.join", "other"), DefaultValue::Required);
    assert_eq!(default("LazyFrame.join", "how"), text("inner"));
    assert_eq!(default("LazyFrame.join", "suffix"), text("_right"));
    assert_eq!(default("LazyFrame.join", "coalesce"), DefaultValue::Unset);
    assert_eq!(
        default("LazyFrame.join", "allow_parallel"),
        DefaultValue::Flag(true)
    );
    assert_eq!(
        default("LazyFrame.join", "force_parallel"),
        DefaultValue::Flag(false)
    );
    // `LazyFrame.filter(*predicates, **constraints)`
    assert_eq!(
        default("LazyFrame.filter", "predicates"),
        DefaultValue::Empty
    );
    assert_eq!(
        default("LazyFrame.filter", "constraints"),
        DefaultValue::Empty
    );
}

#[test]
fn every_default_of_the_inventory_that_python_writes_as_a_literal_is_read() {
    let mut defaults = 0;
    for item in items() {
        for parameter in item.parameters {
            if parameter.starts_with('*') {
                continue;
            }
            let Some((name, python)) = parameter.split_once('=') else {
                continue;
            };
            defaults += 1;
            let first = python.chars().next().expect("a default is written");
            // Only a literal begins with a digit, a sign, a dot or a quote; a name, a call, a
            // tuple or a list begins with something else. What is not read must not be one.
            let literal = first.is_ascii_digit() || matches!(first, '-' | '+' | '.' | '\'' | '"');
            // `math.e` is the one name that is read, as the number it is.
            let literal = literal || python == "math.e";
            let value = read(python);
            assert_eq!(
                value == DefaultValue::Other,
                !literal && !matches!(python, "None" | "True" | "False" | "()" | "{}"),
                "{} {name}={python}: read as {value:?}",
                item.id
            );
        }
    }
    assert!(defaults > 500, "the inventory writes {defaults} defaults");
}

#[test]
fn only_the_count_of_shift_and_the_base_of_log_are_expressions_with_a_default_that_is_a_value() {
    // The document judges an expression to be at its default only for a parameter that counts
    // rows and has a whole number for its default (`Role::Count`), which is `shift(n=1)`. The
    // base of `log` defaults to Euler's number, and an expression of that number is not judged
    // to be at it: the editor draws the base inline, where "same as the default" is not said.
    // Another expression parameter that gets a default has to be looked at there first.
    let mut found: Vec<(&str, &str, Role, DefaultValue)> = entries()
        .iter()
        .flat_map(|entry| {
            entry
                .parameters()
                .iter()
                .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
                .map(move |parameter| {
                    (
                        entry.id().as_str(),
                        parameter.name(),
                        parameter.role(),
                        parameter.default().value(),
                    )
                })
        })
        .filter(|(_, _, _, default)| {
            !matches!(
                default,
                DefaultValue::Unset | DefaultValue::Required | DefaultValue::Empty
            )
        })
        .collect();
    found.sort_by_key(|(entry, parameter, _, _)| (*entry, *parameter));
    assert_eq!(
        found,
        [
            (
                "Expr.log",
                "base",
                Role::Base,
                DefaultValue::Float(std::f64::consts::E)
            ),
            ("Expr.shift", "n", Role::Count, DefaultValue::Integer(1))
        ]
    );
}
