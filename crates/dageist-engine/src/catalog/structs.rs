//! The methods that read, name and make structs, built as Python Polars 2.2.0 builds them: the
//! methods of the `struct` namespace, `polars.struct`, the calls of text and of lists and arrays
//! that give a struct, and `value_counts`.

use polars::prelude::{Expr, PlSmallStr, PolarsError, as_struct};

use super::entries::lazily;
use super::{Argument, Arguments, BuildError};

type Built = Result<Expr, BuildError>;

/// The one name of a field given for the required `parameter`.
fn one_field(arguments: &Arguments, parameter: &'static str) -> Result<String, BuildError> {
    let mut names = arguments.fields(parameter)?;
    match (names.pop(), names.is_empty()) {
        (Some(name), true) => Ok(name),
        (None, _) => Err(BuildError::Missing { parameter }),
        (Some(_), false) => Err(BuildError::OutOfRange {
            parameter,
            expected: "the name of one field",
        }),
    }
}

/// The names given for the required list of texts `parameter`.
fn given_names(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Vec<PlSmallStr>, BuildError> {
    arguments
        .strings(parameter)?
        .map(|names| names.into_iter().map(PlSmallStr::from).collect())
        .ok_or(BuildError::Missing { parameter })
}

/// The number of splits given for the required `parameter`, which Python passes on as an unsigned
/// whole number.
fn parts(arguments: &Arguments, parameter: &'static str) -> Result<usize, BuildError> {
    match arguments.get(parameter) {
        None => Err(BuildError::Missing { parameter }),
        Some(Argument::Integer(value)) => {
            usize::try_from(*value).map_err(|_| BuildError::OutOfRange {
                parameter,
                expected: "a whole number from 0",
            })
        }
        Some(other) => Err(Arguments::wrong(parameter, "a whole number", other)),
    }
}

pub(super) fn struct_field(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.struct_().field_by_name(&one_field(arguments, "name")?))
}

pub(super) fn struct_rename_fields(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .struct_()
        .rename_fields(given_names(arguments, "names")?))
}

pub(super) fn struct_drop(expr: Expr, arguments: &Arguments) -> Built {
    let names = arguments.fields("names")?;
    if arguments.get("names").is_none() {
        return Err(BuildError::Missing { parameter: "names" });
    }
    Ok(expr.struct_().drop(names, arguments.flag("strict", true)?))
}

pub(super) fn struct_json_encode(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.struct_().json_encode())
}

/// `field("*")`, as Python builds `unnest`: a column for each field.
pub(super) fn struct_unnest(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.struct_().field_by_name("*"))
}

pub(super) fn polars_struct(arguments: &Arguments) -> Built {
    if arguments.get("schema").is_some() {
        return Err(BuildError::Polars(PolarsError::InvalidOperation(
            "a step gives no dtype for the fields of a struct: they are the dtypes of what it is made of"
                .into(),
        )));
    }
    lazily(arguments)?;
    let mut exprs = arguments.expressions("exprs")?;
    for (name, value) in arguments.pairs("named_exprs")? {
        match (name, value) {
            (Argument::Columns(names), Argument::Expression(expr)) if names.len() == 1 => {
                exprs.push(expr.alias(names[0].as_str()));
            }
            (_, other) => {
                return Err(Arguments::wrong(
                    "named_exprs",
                    "a name and an expression",
                    &other,
                ));
            }
        }
    }
    if exprs.is_empty() {
        return Err(BuildError::Missing { parameter: "exprs" });
    }
    Ok(as_struct(exprs))
}

pub(super) fn str_split_exact(expr: Expr, arguments: &Arguments) -> Built {
    let by = arguments.required_expression("by")?;
    let n = parts(arguments, "n")?;
    Ok(if arguments.flag("inclusive", false)? {
        expr.str().split_exact_inclusive(by, n)
    } else {
        expr.str().split_exact(by, n)
    })
}

pub(super) fn str_splitn(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .str()
        .splitn(arguments.required_expression("by")?, parts(arguments, "n")?))
}

pub(super) fn list_to_struct(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .list()
        .to_struct(given_names(arguments, "fields")?.into()))
}

pub(super) fn arr_to_struct(expr: Expr, arguments: &Arguments) -> Built {
    let fields = arguments
        .strings("fields")?
        .map(|names| names.into_iter().map(PlSmallStr::from).collect::<Vec<_>>());
    Ok(expr.arr().to_struct(fields.map(Into::into)))
}

pub(super) fn value_counts(expr: Expr, arguments: &Arguments) -> Built {
    let normalize = arguments.flag("normalize", false)?;
    let name = arguments
        .string("name")?
        .unwrap_or_else(|| if normalize { "proportion" } else { "count" }.to_owned());
    Ok(expr.value_counts(
        arguments.flag("sort", false)?,
        arguments.flag("parallel", false)?,
        &name,
        normalize,
    ))
}
