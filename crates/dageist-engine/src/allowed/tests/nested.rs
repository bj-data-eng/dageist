//! The classes of lists and arrays: one for each scalar element dtype the tables cover.

use std::collections::BTreeSet;

use polars::prelude::{DataType, TimeUnit, TimeZone};

use crate::allowed::dtype::{Inner, Representative, ResultDtype, dtype_name};

/// The twenty-two scalar dtypes the tables cover, each as Polars writes it: every whole number,
/// the floats, text, truth, the moments and lengths of time, the dtype of nothing and a decimal.
fn scalars() -> Vec<DataType> {
    vec![
        DataType::Int8,
        DataType::Int16,
        DataType::Int32,
        DataType::Int64,
        DataType::Int128,
        DataType::UInt8,
        DataType::UInt16,
        DataType::UInt32,
        DataType::UInt64,
        DataType::UInt128,
        DataType::Float16,
        DataType::Float32,
        DataType::Float64,
        DataType::String,
        DataType::Boolean,
        DataType::Date,
        DataType::Time,
        DataType::Datetime(TimeUnit::Microseconds, None),
        DataType::Datetime(TimeUnit::Microseconds, Some(TimeZone::UTC)),
        DataType::Duration(TimeUnit::Microseconds),
        DataType::Null,
        DataType::Decimal(10, 2),
    ]
}

fn list(inner: &DataType) -> DataType {
    DataType::List(Box::new(inner.clone()))
}

fn array(inner: &DataType, width: usize) -> DataType {
    DataType::Array(Box::new(inner.clone()), width)
}

#[test]
fn a_list_and_an_array_of_each_scalar_dtype_is_a_class_of_its_own() {
    let mut names = BTreeSet::new();
    for inner in scalars() {
        for nested in [list(&inner), array(&inner, 2)] {
            let class = Representative::of(&nested)
                .unwrap_or_else(|| panic!("{} is a class of the tables", dtype_name(&nested)));
            // The class writes itself as Polars writes the dtype the tables measured it on.
            assert_eq!(class.name(), dtype_name(&nested), "{nested:?}");
            assert_eq!(class.dtype(), nested, "{nested:?}");
            assert_eq!(Representative::of(&class.dtype()), Some(class));
            assert_eq!(Representative::from_name(class.name()), Some(class));
            assert_eq!(Representative::of_name(class.name()), Some(class));
            assert_eq!(
                ResultDtype::from_name(class.name()).class(),
                Some(class),
                "a result written {}",
                class.name()
            );
            assert!(
                names.insert(class.name()),
                "{} is named twice",
                class.name()
            );
        }
    }
    assert_eq!(names.len(), 44, "a list and an array of each of 22 dtypes");
}

#[test]
fn the_names_the_tables_already_hold_keep_their_classes() {
    for name in ["List(Int64)", "List(String)", "Array(Int64, 2)"] {
        let class = Representative::from_name(name).expect("a class");
        assert_eq!(class.name(), name);
    }
    // A list of a whole number other than `Int64` is its own class, and not the list of `Int64`.
    assert_ne!(
        Representative::of(&list(&DataType::Int8)),
        Representative::of(&list(&DataType::Int64))
    );
}

#[test]
fn an_array_is_one_class_whatever_its_width() {
    for inner in scalars() {
        assert_eq!(
            Representative::of(&array(&inner, 5)),
            Representative::of(&array(&inner, 2)),
            "{}",
            dtype_name(&array(&inner, 5))
        );
        assert_eq!(
            Representative::of_name(&dtype_name(&array(&inner, 5))),
            Representative::of(&array(&inner, 2))
        );
    }
    assert!(Representative::of(&array(&DataType::Int64, 2)).is_some());
}

#[test]
fn the_unit_and_the_zone_of_an_element_do_not_make_another_class() {
    for (other, same) in [
        (
            DataType::Datetime(TimeUnit::Nanoseconds, None),
            DataType::Datetime(TimeUnit::Microseconds, None),
        ),
        (
            DataType::Datetime(TimeUnit::Milliseconds, Some(TimeZone::UTC)),
            DataType::Datetime(TimeUnit::Microseconds, Some(TimeZone::UTC)),
        ),
        (
            DataType::Duration(TimeUnit::Nanoseconds),
            DataType::Duration(TimeUnit::Microseconds),
        ),
        (DataType::Decimal(20, 5), DataType::Decimal(10, 2)),
    ] {
        assert!(Representative::of(&list(&same)).is_some());
        assert_eq!(
            Representative::of(&list(&other)),
            Representative::of(&list(&same)),
            "{}",
            dtype_name(&list(&other))
        );
        assert_eq!(
            Representative::of_name(&dtype_name(&list(&other))),
            Representative::of(&list(&same))
        );
    }
}

#[test]
fn a_list_or_an_array_of_a_nested_dtype_has_no_class() {
    let struct_of = DataType::Struct(vec![polars::prelude::Field::new(
        "x".into(),
        DataType::Int64,
    )]);
    for inner in [
        list(&DataType::Int64),
        array(&DataType::Int64, 2),
        struct_of,
    ] {
        for nested in [list(&inner), array(&inner, 2)] {
            assert_eq!(
                Representative::of(&nested),
                None,
                "{} is beyond one level",
                dtype_name(&nested)
            );
            assert_eq!(Representative::of_name(&dtype_name(&nested)), None);
            assert_eq!(ResultDtype::from_name(&dtype_name(&nested)).class(), None);
        }
    }
}

#[test]
fn a_cast_is_allowed_as_polars_allows_it_and_gives_the_dtype_it_is_given_after_any_list_or_array() {
    use polars::prelude::{DataType, Expr, col};

    use crate::allowed::offers::{After, Chosen, Place, RefusalSource, Verdict, offer};
    use crate::allowed::{Context, Outer, Shape};

    use super::probe::measure;
    use super::sample::Frames;
    let cast = crate::catalog::entry("Expr.cast").expect("an entry");
    let mut wrong = Vec::new();
    for receiver in Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
    {
        let place = Place::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: receiver,
                shape: Shape::Many,
            },
        );
        // The dtypes the tables hold a word for: a cast to an array needs rows of its width, which
        // the data decide.
        for given in [
            Representative::List(Inner::Int64),
            Representative::List(Inner::String),
            Representative::Int64,
            Representative::String,
        ] {
            let target: Expr = col("a").strict_cast(given.dtype());
            let frames = Frames::new(receiver, receiver, &DataType::Int64);
            let polars = measure(
                &|frames, empty| Ok(frames.left(empty).select([target.clone()])),
                &frames,
            );
            let verdict = offer(
                cast,
                &place,
                &Chosen::default().with_options(&[("dtype", given.name())]),
            );
            let said = match (&verdict, &polars) {
                // What a rule refuses is stricter than Polars, and is not asked of it.
                (Verdict::Refused(refusal), _)
                    if matches!(refusal.source(), RefusalSource::Rule(_)) =>
                {
                    None
                }
                (Verdict::Allowed(allowance), Ok(_)) => {
                    (allowance.result().map(crate::allowed::ResultDtype::name)
                        != Some(given.name()))
                    .then(|| format!("gives {:?}", allowance.result()))
                }
                (Verdict::Allowed(_), Err(reason)) => {
                    Some(format!("is allowed and Polars refuses it: {}", reason.text))
                }
                (Verdict::Refused(refusal), Ok(_)) => Some(format!(
                    "is refused and Polars allows it: {}",
                    refusal.reason()
                )),
                (Verdict::Refused(_), Err(_)) => None,
            };
            if let Some(said) = said {
                wrong.push(format!("a cast of {receiver} to {given} {said}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} casts differ from Polars:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_refusal_read_from_the_class_that_stands_for_a_list_names_the_dtype_that_was_asked() {
    use crate::allowed::offers::{After, Chosen, Place, Verdict, offer};
    use crate::allowed::{Context, Outer, Shape};
    // Polars refuses `approx_n_unique` of an array, in words that name the dtype of the array.
    let entry = crate::catalog::entry("Expr.approx_n_unique").expect("an entry");
    for asked in [
        Representative::Array(Inner::Date),
        Representative::Array(Inner::DatetimeZoned),
        Representative::Array(Inner::String),
    ] {
        let place = Place::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: asked,
                shape: Shape::Many,
            },
        );
        let Verdict::Refused(refusal) = offer(entry, &place, &Chosen::default()) else {
            panic!("Polars refuses it of {asked}");
        };
        let measured = Representative::Array(Inner::Int64).dtype().to_string();
        assert!(
            refusal.reason().contains(&asked.dtype().to_string())
                && !refusal.reason().contains(&measured),
            "the refusal of {asked} names {measured} and not it: {}",
            refusal.reason()
        );
    }
}

#[test]
fn a_method_that_depends_on_the_element_is_refused_by_a_rule_after_a_class_it_is_not_measured_on() {
    use crate::allowed::offers::{After, Chosen, Place, RefusalSource, Verdict, offer};
    use crate::allowed::table::{SENSITIVE, measured_classes};
    use crate::allowed::{Context, Outer, Shape};
    let mut allowed = Vec::new();
    for id in SENSITIVE {
        let entry = crate::catalog::entry(id).expect("an entry");
        for class in Representative::ALL
            .into_iter()
            .filter(|class| class.is_nested() && !measured_classes(id).contains(class))
        {
            let place = Place::new(
                Context::step(Outer::Select),
                After::Value {
                    dtype: class,
                    shape: Shape::Many,
                },
            );
            // Polars is not asked of what the tables do not measure, and may panic on it: the
            // call is refused, and not allowed with nothing said of it.
            let by_a_rule = matches!(
                offer(entry, &place, &Chosen::default()),
                Verdict::Refused(refusal) if matches!(refusal.source(), RefusalSource::Rule(_))
            );
            if !by_a_rule {
                allowed.push(format!("{id} after {class}"));
            }
        }
    }
    assert!(
        allowed.is_empty(),
        "{} calls follow a class they are not measured on without a rule that refuses them: {:?}",
        allowed.len(),
        &allowed[..allowed.len().min(12)]
    );
}

#[test]
fn a_fill_by_a_strategy_is_refused_after_a_class_it_is_not_measured_on() {
    use crate::allowed::offers::{After, Chosen, Place, RefusalSource, Verdict, offer};
    use crate::allowed::{Context, Outer, Shape};
    // A fill takes a strategy in place of a value, so no line of the dtype table is asked of it.
    let entry = crate::catalog::entry("Expr.fill_null").expect("an entry");
    for class in [
        Representative::List(Inner::Duration),
        Representative::Array(Inner::Null),
        Representative::List(Inner::Time),
    ] {
        let place = Place::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: class,
                shape: Shape::Many,
            },
        );
        for strategy in ["forward", "mean", "zero"] {
            let verdict = offer(
                entry,
                &place,
                &Chosen::default().with_options(&[("strategy", strategy)]),
            );
            assert!(
                matches!(
                    &verdict,
                    Verdict::Refused(refusal) if matches!(refusal.source(), RefusalSource::Rule(_))
                ),
                "a fill by {strategy} after {class}: {verdict:?}"
            );
        }
    }
}
