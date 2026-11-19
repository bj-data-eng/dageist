//! How a typed number or text is shown to the editor. Every expected text is
//! worked out by hand from how Python writes its values, not read from what the
//! engine gives.

use dageist_engine::allowed::Literal;

#[test]
fn a_number_is_shown_as_python_writes_it() {
    // Python's `repr`: a whole float keeps its `.0`, the shortest digits that read back are
    // used, and an exponent only appears below 1e-4 and from 1e16, with a sign and two digits.
    for (number, words) in [
        (1.0, "1.0"),
        (0.1, "0.1"),
        (1e16, "1e+16"),
        (1e20, "1e+20"),
        (123_456_789_012_345.0, "123456789012345.0"),
        (1e-5, "1e-05"),
        (0.0001, "0.0001"),
        (-2.5e-7, "-2.5e-07"),
        (1.5e300, "1.5e+300"),
        (-0.0, "-0.0"),
        (100.0, "100.0"),
    ] {
        assert_eq!(Literal::Float(number).shown(), words, "{number:e}");
    }
    // A whole number is its digits.
    for number in [0, 7, -3, i64::MAX, i64::MIN] {
        assert_eq!(Literal::Integer(number).shown(), number.to_string());
    }
}

#[test]
fn text_is_shown_as_it_is_with_nothing_quoted_or_escaped() {
    // The editor draws the quotation marks of a block that holds text, so nothing is added or
    // escaped, and the white space it was typed with stays.
    for typed in ["within", " a b ", "say \"hi\"\n\\ é\t", "", "1.0"] {
        assert_eq!(Literal::String(typed.to_owned()).shown(), typed);
    }
}
