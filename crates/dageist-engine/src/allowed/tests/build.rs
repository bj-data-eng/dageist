//! Building calls through the catalogue, as a document's steps are built.

use polars::prelude::{DataType, Expr, LazyFrame};

use crate::catalog::{self, Argument, Arguments, Build, BuildError, Conditional, Entry, Kind};

/// The pattern the harness gives a method that must be given one: a regular expression with a
/// named group, which every method that takes a pattern accepts and which gives a struct of one
/// field of text where it names the fields of what it gives.
pub(super) const PATTERN: &str = "(?<g>a)";

/// The arguments of `entry` that are patterns, each the pattern of the harness, for the
/// parameters that must be given one.
pub(super) fn patterns(entry: &Entry) -> Vec<(&'static str, Argument)> {
    entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.kind() == Kind::Pattern && parameter.is_needed())
        .map(|parameter| (parameter.name(), Argument::Pattern(PATTERN.to_owned())))
        .collect()
}

/// The arguments of `entry` that are no expressions, choices or patterns and must be given, each of a
/// value any call accepts: a number of splits, the name of a field of the struct the tables measure,
/// which has the fields `x` and `y`, and two names for the two fields of that struct or the first
/// two values of a list.
pub(super) fn required_values(entry: &Entry) -> Vec<(&'static str, Argument)> {
    entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.is_needed())
        .filter_map(|parameter| {
            let one = match parameter.kind() {
                Kind::Value(catalog::ValueType::Integer) => Argument::Integer(2),
                Kind::Field => Argument::Field("x".to_owned()),
                Kind::Value(catalog::ValueType::String) => match parameter.shape() {
                    catalog::Shape::Many => {
                        return Some((
                            parameter.name(),
                            Argument::Many(vec![
                                Argument::String("a".to_owned()),
                                Argument::String("b".to_owned()),
                            ]),
                        ));
                    }
                    _ => return None,
                },
                _ => return None,
            };
            Some((
                parameter.name(),
                match parameter.shape() {
                    catalog::Shape::Many => Argument::Many(vec![one]),
                    _ => one,
                },
            ))
        })
        .collect()
}

/// The arguments of `entry` that are choices that must be given, each the first word of the
/// choice: any word makes the call run, and the option table says what each does.
pub(super) fn required_choices(entry: &Entry) -> Vec<(&'static str, Argument)> {
    entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.is_needed())
        .filter_map(|parameter| match parameter.kind() {
            Kind::Choice(words) => Some((parameter.name(), Argument::Choice(words[0].to_owned()))),
            _ => None,
        })
        .collect()
}

/// The entry named `id`.
pub(super) fn entry(id: &str) -> &'static Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has no entry {id}"))
}

fn arguments(entry: &Entry, given: Vec<(&'static str, Argument)>) -> Result<Arguments, BuildError> {
    entry.arguments(given)
}

/// `entry` applied to `receiver`.
pub(super) fn method(
    entry: &Entry,
    receiver: Expr,
    given: Vec<(&'static str, Argument)>,
) -> Result<Expr, BuildError> {
    let Build::Expression(build) = entry.build() else {
        panic!("{} is not a method", entry.id().as_str());
    };
    build(receiver, &arguments(entry, given)?)
}

/// The function `id` called with `given`.
pub(super) fn function(id: &str, given: Vec<(&'static str, Argument)>) -> Result<Expr, BuildError> {
    let entry = entry(id);
    let Build::Function(build) = entry.build() else {
        panic!("{id} is not a function");
    };
    build(&arguments(entry, given)?)
}

/// The step `id` applied to `frame`.
pub(super) fn step(
    id: &str,
    frame: LazyFrame,
    given: Vec<(&'static str, Argument)>,
) -> Result<LazyFrame, BuildError> {
    let entry = entry(id);
    let Build::Frame(build) = entry.build() else {
        panic!("{id} is not a step");
    };
    build(frame, &arguments(entry, given)?)
}

/// The chain that groups `frame` as `group_by` is given `given`, continues with the calls of
/// `links` and is finished by `finish`, each given its arguments.
pub(super) fn grouped(
    frame: LazyFrame,
    given: Vec<(&'static str, Argument)>,
    links: Vec<(&str, Vec<(&'static str, Argument)>)>,
    finish: (&str, Vec<(&'static str, Argument)>),
) -> Result<LazyFrame, BuildError> {
    let start = entry("LazyFrame.group_by");
    let Build::Grouping(start_build) = start.build() else {
        panic!("group_by starts a chain");
    };
    let mut group = start_build(frame, &arguments(start, given)?)?;
    for (id, given) in links {
        let link = entry(id);
        let Build::GroupedStep(build) = link.build() else {
            panic!("{id} does not continue a chain");
        };
        group = build(group, &arguments(link, given)?)?;
    }
    let (id, given) = finish;
    let end = entry(id);
    let Build::GroupedEnd(build) = end.build() else {
        panic!("{id} does not finish a chain");
    };
    build(group, &arguments(end, given)?)
}

/// `col(name)`.
pub(super) fn column(name: &str) -> Expr {
    function(
        "polars.col",
        vec![("name", Argument::Columns(vec![name.to_owned()]))],
    )
    .expect("a column name builds")
}

/// `element()`.
pub(super) fn element() -> Expr {
    function("polars.element", Vec::new()).expect("element builds")
}

/// A whole number.
pub(super) fn integer(value: i64) -> Expr {
    function("polars.lit", vec![("value", Argument::Integer(value))]).expect("a number builds")
}

/// A flag.
pub(super) fn flag(value: bool) -> Expr {
    function("polars.lit", vec![("value", Argument::Flag(Some(value)))]).expect("a flag builds")
}

/// A null of `dtype`.
pub(super) fn null_of(dtype: &DataType) -> Result<Expr, BuildError> {
    function(
        "polars.lit",
        vec![
            ("value", Argument::Flag(None)),
            ("dtype", Argument::Dtype(dtype.clone())),
        ],
    )
}

/// The whole number 1 as a literal of `dtype`, which Polars refuses to make
/// of a dtype it cannot cast a number to.
pub(super) fn one_of(dtype: &DataType) -> Result<Expr, BuildError> {
    function(
        "polars.lit",
        vec![
            ("value", Argument::Integer(1)),
            ("dtype", Argument::Dtype(dtype.clone())),
        ],
    )
}

/// One expression as an argument.
pub(super) fn expression(expr: Expr) -> Argument {
    Argument::Expression(expr)
}

/// A list of expressions as an argument.
pub(super) fn expressions(exprs: Vec<Expr>) -> Argument {
    Argument::Many(exprs.into_iter().map(Argument::Expression).collect())
}

/// The calls that make a condition, from `when` to `otherwise`, such that
/// every link of the catalogue stands in one of them.
const CONDITIONS: [&[&str]; 3] = [
    &["polars.when", "When.then", "Then.otherwise"],
    &[
        "polars.when",
        "When.then",
        "Then.when",
        "ChainedWhen.then",
        "ChainedThen.otherwise",
    ],
    &[
        "polars.when",
        "When.then",
        "Then.when",
        "ChainedWhen.then",
        "ChainedThen.when",
        "ChainedWhen.then",
        "ChainedThen.otherwise",
    ],
];

/// The shortest complete condition that has the link `id` in it, every
/// predicate and value `true`.
pub(super) fn condition_with(id: &str) -> Result<Expr, BuildError> {
    let links = CONDITIONS
        .iter()
        .find(|links| links.contains(&id))
        .unwrap_or_else(|| panic!("no condition holds {id}"));
    let mut state: Option<Conditional> = None;
    for link in *links {
        let entry = entry(link);
        let given = if entry.parameters().iter().any(|p| p.name() == "statement") {
            vec![("statement", expression(flag(true)))]
        } else {
            vec![("predicates", expressions(vec![flag(true)]))]
        };
        let arguments = arguments(entry, given)?;
        match (entry.build(), state.take()) {
            (Build::Condition(build), None) => state = Some(build(&arguments)?),
            (Build::ConditionStep(build), Some(after)) => state = Some(build(after, &arguments)?),
            (Build::ConditionEnd(build), Some(after)) => return build(after, &arguments),
            _ => panic!("{link} cannot follow the links before it"),
        }
    }
    panic!("a condition has an end")
}
