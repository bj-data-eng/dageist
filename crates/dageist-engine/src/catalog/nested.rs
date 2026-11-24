//! The methods of the `list` and the `arr` namespaces that take no expression argument, built as
//! Python Polars 2.1.0 builds them: the ones that Python gives to Rust directly are the
//! namespace's own, and the rest are an evaluation of a method of one list's elements, over
//! `element()`: `n_unique`, `any`, `all`, `reverse` of a list and `unique`; `first` and `last` are the value
//! at the first and the last place, which is a null where a list has none.

use polars::prelude::{Expr, SortOptions, element, lit};

use super::{Arguments, BuildError, entries::ddof};

type Built = Result<Expr, BuildError>;

fn sorted(arguments: &Arguments) -> Result<SortOptions, BuildError> {
    Ok(SortOptions {
        descending: arguments.flag("descending", false)?,
        nulls_last: arguments.flag("nulls_last", false)?,
        multithreaded: true,
        maintain_order: false,
        limit: None,
    })
}

/// The elements of one list, each different one once, as `unique` is built.
fn each_once(arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(if arguments.flag("maintain_order", false)? {
        element().unique_stable()
    } else {
        element().unique()
    })
}

pub(super) fn list_len(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().len())
}

pub(super) fn list_sum(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().sum())
}

pub(super) fn list_mean(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().mean())
}

pub(super) fn list_median(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().median())
}

pub(super) fn list_min(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().min())
}

pub(super) fn list_max(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().max())
}

pub(super) fn list_arg_max(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().arg_max())
}

pub(super) fn list_arg_min(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().arg_min())
}

pub(super) fn list_std(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.list().std(ddof(arguments)?))
}

pub(super) fn list_var(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.list().var(ddof(arguments)?))
}

pub(super) fn list_sort(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.list().sort(sorted(arguments)?))
}

pub(super) fn list_first(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().first())
}

pub(super) fn list_last(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().last())
}

pub(super) fn list_n_unique(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().agg(element().n_unique()))
}

pub(super) fn list_any(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .list()
        .agg(element().any(arguments.flag("ignore_nulls", true)?)))
}

pub(super) fn list_all(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .list()
        .agg(element().all(arguments.flag("ignore_nulls", true)?)))
}

pub(super) fn list_reverse(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().eval(element().reverse()))
}

pub(super) fn list_unique(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.list().eval(each_once(arguments)?))
}

pub(super) fn list_drop_nulls(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.list().drop_nulls())
}

pub(super) fn arr_len(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().len())
}

pub(super) fn arr_sum(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().sum())
}

pub(super) fn arr_mean(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().mean())
}

pub(super) fn arr_median(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().median())
}

pub(super) fn arr_min(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().min())
}

pub(super) fn arr_max(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().max())
}

pub(super) fn arr_arg_max(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().arg_max())
}

pub(super) fn arr_arg_min(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().arg_min())
}

pub(super) fn arr_std(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.arr().std(ddof(arguments)?))
}

pub(super) fn arr_var(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.arr().var(ddof(arguments)?))
}

pub(super) fn arr_sort(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.arr().sort(sorted(arguments)?))
}

pub(super) fn arr_first(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().get(lit(0_i64), true))
}

pub(super) fn arr_last(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().get(lit(-1_i64), true))
}

pub(super) fn arr_n_unique(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().agg(element().n_unique()))
}

pub(super) fn arr_any(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .arr()
        .agg(element().any(arguments.flag("ignore_nulls", true)?)))
}

pub(super) fn arr_all(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr
        .arr()
        .agg(element().all(arguments.flag("ignore_nulls", true)?)))
}

pub(super) fn arr_unique(expr: Expr, arguments: &Arguments) -> Built {
    Ok(expr.arr().eval(each_once(arguments)?, true))
}

pub(super) fn arr_to_list(expr: Expr, _arguments: &Arguments) -> Built {
    Ok(expr.arr().to_list())
}
