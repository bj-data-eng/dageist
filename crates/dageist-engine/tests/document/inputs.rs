//! The inputs a document's steps can name.

use dageist_engine::document::{DuplicateInput, Inputs};
use polars::prelude::*;

#[test]
fn a_name_belongs_to_one_input_whatever_its_kind() {
    let table = || df!("a" => [1_i64]).expect("a table").lazy();
    let value = || Scalar::new(DataType::Int64, AnyValue::Int64(1));
    let list = || Series::new("a".into(), [1_i64]);

    let mut inputs = Inputs::new();
    inputs.add_table("taken", table()).expect("a table");
    for result in [
        inputs.add_table("taken", table()),
        inputs.add_value("taken", value()),
        inputs.add_list("taken", list()),
    ] {
        assert_eq!(result, Err(DuplicateInput("taken".to_owned())));
    }
    inputs.add_value("value", value()).expect("a value");
    inputs.add_list("list", list()).expect("a list");
    assert_eq!(
        inputs.add_table("value", table()),
        Err(DuplicateInput("value".to_owned()))
    );
    assert!(
        DuplicateInput("taken".to_owned())
            .to_string()
            .contains("taken")
    );
}
