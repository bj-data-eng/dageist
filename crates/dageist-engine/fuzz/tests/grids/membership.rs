//! `is_in` and `implode` held to Polars over grids, both ways.
//!
//! A value of every column of the world, which holds every class the tables know and the dtypes
//! they have no class of, is looked up in every form that can be given for `other`: a column of
//! every class and dtype, each column imploded once and twice, a literal of every class the editor
//! types in and each imploded, a null typed as every class and untyped, a literal followed by each
//! call that keeps its dtype and imploded, a column of empty lists, and a connected list of every
//! class as the run builds it, gathered into one list. Each is asked with `nulls_equal` both ways,
//! as a value and as a join's key, where the walk folds what it takes for an aggregate; the rows
//! of each step are asked too.
//!
//! The labels of the value and of the form begin with how deeply the dtype nests, and a question
//! mark where no class of the tables is the dtype, which the lines name.
//!
//! `implode` is asked on a column of every dtype, imploded once and twice, with `maintain_order`
//! each way, for where Polars refuses it and for the dtype it gives at each place.

use dageist_engine::allowed::{Context, Outer, Representative};
use dageist_engine::catalog::{self, Kind as ParameterKind};
use dageist_engine_fuzz::axes::{self, Effect};
use dageist_engine_fuzz::doc::{call, chain, column, column_then, flag, many, then};
use dageist_engine_fuzz::judge::{Case, Connections, Depth, Kind, Question};
use dageist_engine_fuzz::literals;
use dageist_engine_fuzz::places;
use dageist_engine_fuzz::world::{Column, World};
use dageist_fuzz::grid::{Choices, Grid, Known, Pattern};
use polars::prelude::{DataType, Series};
use serde_json::{Value, json};

/// The label of a value or a form that gives `dtype`: how deeply it nests, and a question mark
/// before that where no class of the tables is the dtype.
fn labelled(dtype: &DataType, what: &str) -> String {
    let level = dtype.nesting_level();
    if Representative::of(dtype).is_some() {
        format!("{level} {what}")
    } else {
        format!("?{level} {what}")
    }
}

fn gathered(dtype: &DataType) -> DataType {
    DataType::List(Box::new(dtype.clone()))
}

fn implode() -> Value {
    call("Expr.implode", json!({}))
}

/// A form of what is looked up in: the expression the step is given, and the list connected for
/// it where it is one.
#[derive(Clone)]
struct Form {
    other: Value,
    list: Option<(String, Series)>,
}

impl Form {
    const fn of(other: Value) -> Self {
        Self { other, list: None }
    }
}

/// Every form `other` is given in.
fn forms() -> Choices<Form> {
    let world = World::get();
    let mut axis = Choices::new("form");
    for (label, column) in world.columns("column").iter() {
        let dtype = &column.dtype;
        axis = axis
            .with(
                labelled(dtype, &format!("a column of {label}")),
                Form::of(self::column(&column.name)),
            )
            .with(
                labelled(&gathered(dtype), &format!("a column of {label} imploded")),
                Form::of(column_then(&column.name, vec![implode()])),
            )
            .with(
                labelled(
                    &gathered(&gathered(dtype)),
                    &format!("a column of {label} imploded twice"),
                ),
                Form::of(column_then(&column.name, vec![implode(), implode()])),
            );
    }
    for left in world.columns("column").left_out() {
        axis = axis.leave_out(format!("a column of {}", left.label), left.reason.clone());
    }
    for class in Representative::ALL {
        let dtype = class.dtype();
        match literals::one_of(class) {
            Some(literal) => {
                axis = axis
                    .with(
                        labelled(&dtype, &format!("a literal of {class}")),
                        Form::of(literal.expression()),
                    )
                    .with(
                        labelled(&gathered(&dtype), &format!("a literal of {class} imploded")),
                        Form::of(chain(literal.call(), vec![implode()])),
                    );
            }
            None => {
                axis = axis.leave_out(
                    format!("a literal of {class}"),
                    "the editor types in no literal of a nested class or a struct",
                );
            }
        }
        match literals::null_of(class) {
            Some(null) => {
                axis = axis.with(
                    labelled(&dtype, &format!("a null typed as {class}")),
                    Form::of(null.expression()),
                );
            }
            None => {
                axis = axis.leave_out(
                    format!("a null typed as {class}"),
                    "the editor types in no null of a nested class or a struct",
                );
            }
        }
        match world.list_of(class) {
            Some((name, list)) => {
                axis = axis.with(
                    labelled(&gathered(&dtype), &format!("a connected list of {class}")),
                    Form {
                        other: chain(json!({ "input": name }), Vec::new()),
                        list: Some((name, list)),
                    },
                );
            }
            None => {
                axis = axis.leave_out(
                    format!("a connected list of {class}"),
                    "no list of a struct is connected, nor of a class whose values cannot be made",
                );
            }
        }
    }
    // A literal that a call is made of, and then gathered: Polars folds some of these into a
    // literal, which a join's key may hold, and not others.
    let whole = literals::one_of(Representative::Int64).expect("a whole number typed in");
    let int64 = world.column("Int64");
    let calls = axes::calls_of_run(axes::calls_that(
        "call",
        Effect::Keeps,
        &[Representative::Int64],
    ));
    for (label, entry) in calls.iter() {
        if let Ok(called) = axes::call_of(entry, Some(int64)) {
            axis = axis.with(
                labelled(
                    &gathered(&DataType::Int64),
                    &format!("a literal of Int64 then {label}, imploded"),
                ),
                Form::of(chain(whole.call(), vec![called, implode()])),
            );
        }
    }
    axis.with(
        labelled(&DataType::Null, "a null"),
        Form::of(chain(
            call("polars.lit", json!({ "value": { "flag": null } })),
            Vec::new(),
        )),
    )
    .leave_out(
        "a list typed in",
        "the editor has no kind for a list typed in, fitting, unfitting or empty",
    )
}

/// The values, each a column of the world, labelled by how deeply it nests.
fn receivers(name: &'static str) -> Choices<Column> {
    World::get()
        .columns(name)
        .relabel(|label, column| labelled(&column.dtype, label))
}

/// Whether `is_in` is asked as a value or as a join's key. A select stands for every place of a
/// value: the walk judges `is_in` by what it gives, which only a join's key holds to be elementwise.
fn value_or_key() -> Choices<Context> {
    places::step_places("place")
        .keep_unless(|_, context| {
            (!matches!(context.outer(), Outer::Select | Outer::JoinLeft)).then(|| {
                "the walk judges is_in by what it gives, the same at every place of a value, \
                 which a select stands for"
                    .to_owned()
            })
        })
        .relabel(|_, context| {
            if context.outer() == Outer::Select {
                "select".to_owned()
            } else {
                "key".to_owned()
            }
        })
}

/// The flags of a parameter: true and false.
fn flags(name: &'static str) -> Choices<bool> {
    Choices::of(name, [("false", false), ("true", true)])
}

#[derive(Default)]
struct Point {
    receiver: Option<Column>,
    form: Option<Form>,
    nulls_equal: bool,
    place: Option<Context>,
}

fn is_in_case(point: &Point) -> Result<Case, String> {
    let receiver = point.receiver.as_ref().ok_or("no receiver")?;
    let form = point.form.as_ref().ok_or("no form")?;
    let looked_up = column_then(
        &receiver.name,
        vec![call(
            "Expr.is_in",
            json!({ "other": form.other, "nulls_equal": flag(point.nulls_equal) }),
        )],
    );
    let world = World::get();
    let mut connections = Connections::new();
    if let Some((name, list)) = &form.list {
        connections = connections.list(name, list.clone());
    }
    match point.place.expect("a place").outer() {
        Outer::Select => Case::of_step(
            "LazyFrame.select",
            json!({ "exprs": many(vec![looked_up]) }),
            world.frame().clone(),
            connections,
        ),
        // The key of the other side is its flag column, of the dtype `is_in` gives.
        _ => Case::of_step(
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![looked_up]),
                "right_on": many(vec![column("rf")]),
            }),
            world.frame().clone(),
            connections.table("other", world.other().clone()),
        ),
    }
}

/// What the grid of `is_in` names: the deprecated form, and the dtypes no class of the tables is.
fn known() -> Vec<Known<Kind>> {
    let mut known = Vec::new();
    // The deprecated form: what is looked up in is nested as the value is, which Polars accepts
    // with a warning that it is ambiguous (pola-rs/polars#22149). The rule refuses it.
    for level in ["0 ", "1 "] {
        known.push(
            Known::new(
                Kind::WalkRefusesOnly,
                Pattern::new()
                    .starts("receiver", level)
                    .starts("form", level),
                "the deprecated form, nested as the value is, which the rule is-in-takes-a-list \
                 refuses and Polars accepts with a warning (pola-rs/polars#22149)",
            )
            .saying("rule is-in-takes-a-list"),
        );
    }
    known.push(
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("form", "?"),
            "what is looked up in is of a dtype with no class of the tables (a list of lists, of \
             structs, a binary, a categorical, an enum), so the walk cannot tell that Polars \
             refuses it and leaves it to the step's run (#237)",
        )
        .saying("cannot check for"),
    );
    known.push(
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("receiver", "?"),
            "the value is of a dtype with no class of the tables (a binary, a categorical, an \
             enum), so the walk cannot tell what it may be looked up in and leaves it to the \
             step's run (#237)",
        )
        .saying("cannot check for"),
    );
    // The deprecated form where one side is of such a dtype: the walk does not know the dtype, so
    // it cannot tell that the nesting is equal, and only the step's run refuses it by the rule.
    // Polars injects an implode for the deprecated form, which leaves a join's key not elementwise.
    for (receiver, form) in [("?0", "?0"), ("?0", "0 "), ("0 ", "?0")] {
        known.push(
            Known::new(
                Kind::PolarsRefusesOnly,
                Pattern::new()
                    .is("place", "key")
                    .starts("receiver", receiver)
                    .starts("form", form),
                "the deprecated form with a dtype of no class on one side: Polars gathers what is \
                 looked up in into a list for it, which leaves a join's key not elementwise, and \
                 the walk, which does not know the dtype, cannot tell that it is the deprecated \
                 form (#237)",
            )
            .saying("elementwise"),
        );
    }
    known
}

#[test]
fn is_in_given_each_form_of_other_is_what_polars_says_as_a_value_and_as_a_key() {
    let grid = Grid::<Point>::new("is-in")
        .axis(receivers("receiver"), |point, receiver| {
            point.receiver = Some(receiver.clone());
        })
        .axis(forms(), |point, form| point.form = Some(form.clone()))
        .axis(flags("nulls_equal"), |point, equal| {
            point.nulls_equal = *equal
        })
        .axis(value_or_key(), |point, place| point.place = Some(*place));
    let question = Question::any_refusal().at(Depth::Rows);
    grid.judge(|point| question.judge(is_in_case(point)))
        .assert_agree(&known(), 10_000);
}

/// The column `implode` follows, how many times, and what `maintain_order` is given.
#[derive(Default)]
struct Imploded {
    receiver: Option<Column>,
    times: usize,
    order: Option<bool>,
}

/// The settings of `implode`'s flag `maintain_order`: its default, and each way.
fn maintain_order() -> Choices<Option<bool>> {
    let implode = catalog::entry("Expr.implode").expect("implode");
    let flag = implode
        .parameters()
        .iter()
        .find(|parameter| parameter.kind() == ParameterKind::Flag)
        .expect("a flag of implode");
    Choices::of(
        flag.name(),
        [
            ("default", None),
            ("true", Some(true)),
            ("false", Some(false)),
        ],
    )
}

#[test]
fn implode_is_refused_where_polars_refuses_it_and_gives_the_dtype_polars_gives() {
    let grid = Grid::<Imploded>::new("implode")
        .axis(World::get().columns("receiver"), |point, receiver| {
            point.receiver = Some(receiver.clone());
        })
        .axis(
            Choices::of("times", [("once", 1_usize), ("twice", 2)]),
            |point, times| point.times = *times,
        )
        .axis(maintain_order(), |point, order| point.order = *order);
    let question = Question::any_refusal().at(Depth::Rows).with_dtypes();
    grid.judge(|point| {
        question.judge((|| {
            let receiver = point.receiver.as_ref().ok_or("no receiver")?;
            let link = match point.order {
                None => implode(),
                Some(keep) => call("Expr.implode", json!({ "maintain_order": flag(keep) })),
            };
            let expression = then(column(&receiver.name), vec![link; point.times]);
            Case::of_step(
                "LazyFrame.select",
                json!({ "exprs": many(vec![expression]) }),
                World::get().frame().clone(),
                Connections::new(),
            )
        })())
    })
    .assert_agree(&[], 100);
}
