//! The JSON a document is read from is read by RFC 8259, strictly, and a
//! syntax error names the line and column of the first character that is wrong.
//! Columns count characters, from 1; a line ends at `\n`.

use dageist_engine::document::{Document, ReadError, SyntaxProblem};

/// The syntax error reading `text` gives.
fn syntax(text: &str) -> (SyntaxProblem, usize, usize) {
    match Document::read(text) {
        Err(ReadError::Syntax(error)) => (error.problem().clone(), error.line(), error.column()),
        other => panic!("expected a syntax error for {text:?}, got {other:?}"),
    }
}

#[test]
fn a_trailing_comma_is_refused_where_it_stands() {
    assert_eq!(
        syntax("{\n  \"version\": 1,\n  \"steps\": [1,]\n}"),
        (SyntaxProblem::Unexpected(']'), 3, 15)
    );
    assert_eq!(
        syntax(r#"{"version":1,}"#),
        (SyntaxProblem::Unexpected('}'), 1, 14)
    );
}

#[test]
fn a_leading_zero_is_refused_at_the_start_of_the_number() {
    for (text, column) in [
        (r#"{"version":01}"#, 12),
        (r#"{"version":-01}"#, 12),
        (r#"{"version":00}"#, 12),
        (r#"{"version":[0.5,007]}"#, 17),
    ] {
        assert_eq!(
            syntax(text),
            (SyntaxProblem::LeadingZero, 1, column),
            "{text}"
        );
    }
}

#[test]
fn what_json_has_no_number_for_is_refused() {
    for (text, found, column) in [
        (r#"{"version":NaN}"#, 'N', 12),
        (r#"{"version":Infinity}"#, 'I', 12),
        (r#"{"version":-Infinity}"#, 'I', 13),
        (r#"{"version":+1}"#, '+', 12),
        (r#"{"version":.5}"#, '.', 12),
        (r#"{"version":-}"#, '}', 13),
        (r#"{"version":1.}"#, '}', 14),
        (r#"{"version":1e}"#, '}', 14),
        (r#"{"version":1e+}"#, '}', 15),
        (r#"{"version":0x10}"#, 'x', 13),
        (r#"{"version":1_0}"#, '_', 13),
    ] {
        assert_eq!(
            syntax(text),
            (SyntaxProblem::Unexpected(found), 1, column),
            "{text}"
        );
    }
}

#[test]
fn an_unescaped_control_character_in_a_string_is_refused() {
    for control in ['\t', '\n', '\u{0}', '\u{1f}'] {
        let text = format!("{{\"a\":\"x{control}y\"}}");
        assert_eq!(
            syntax(&text),
            (SyntaxProblem::ControlCharacter(control), 1, 8),
            "{control:?}"
        );
    }
}

#[test]
fn a_string_may_hold_what_is_not_a_control_character_unescaped() {
    // DEL, a quote's neighbour, and characters beyond ASCII need no escape.
    let text = "{\"version\":1,\"steps\":[],\"\u{7f}é😀\":0}";
    assert!(matches!(Document::read(text), Err(ReadError::Refused(_))));
}

#[test]
fn an_invalid_escape_is_refused() {
    for (text, problem, column) in [
        (r#"{"a":"\x"}"#, SyntaxProblem::InvalidEscape('x'), 8),
        (r#"{"a":"\'"}"#, SyntaxProblem::InvalidEscape('\''), 8),
        (r#"{"a":"\U0041"}"#, SyntaxProblem::InvalidEscape('U'), 8),
        (r#"{"a":"\u12G4"}"#, SyntaxProblem::InvalidUnicodeEscape, 11),
        (r#"{"a":"\u12"}"#, SyntaxProblem::InvalidUnicodeEscape, 11),
        (r#"{"a":"\u"#, SyntaxProblem::InvalidUnicodeEscape, 9),
        (r#"{"a":"\"#, SyntaxProblem::EndOfText, 8),
    ] {
        assert_eq!(syntax(text), (problem, 1, column), "{text}");
    }
}

#[test]
fn half_a_surrogate_pair_is_refused_at_its_escape() {
    for text in [
        r#"{"a":"\ud800"}"#,
        r#"{"a":"\ud800x"}"#,
        r#"{"a":"\ud800\u0041"}"#,
        r#"{"a":"\ud800\ud800"}"#,
        r#"{"a":"\udc00"}"#,
        r#"{"a":"\udfff\ud800"}"#,
    ] {
        assert_eq!(syntax(text), (SyntaxProblem::LoneSurrogate, 1, 7), "{text}");
    }
}

#[test]
fn escapes_and_surrogate_pairs_read_as_the_characters_they_name() {
    let text = r#"{"version":1,"steps":[{"id":"a\u00e9\ud83d\ude00\n\/\"\\\b\f\r\t\u0000\uD83D\uDE00","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[]}}}}]}"#;
    let document = Document::read(text).expect("a document");
    assert_eq!(
        document.steps()[0].id().as_str(),
        "a\u{e9}\u{1f600}\n/\"\\\u{8}\u{c}\r\t\u{0}\u{1f600}"
    );
    // Written back with the shortest escapes JSON has, and read back as itself.
    let written = document.write();
    assert!(
        written.contains(r#""id":"aé😀\n/\"\\\b\f\r\t\u0000😀""#),
        "{written}"
    );
    assert_eq!(
        Document::read(&written).expect("the text just written"),
        document
    );
}

#[test]
fn text_after_the_value_is_refused() {
    assert_eq!(
        syntax(r#"{"version":1,"steps":[]} x"#),
        (SyntaxProblem::TrailingText('x'), 1, 26)
    );
    assert_eq!(
        syntax("{\"version\":1,\"steps\":[]}\n{}"),
        (SyntaxProblem::TrailingText('{'), 2, 1)
    );
    assert_eq!(syntax("{} {}"), (SyntaxProblem::TrailingText('{'), 1, 4));
}

#[test]
fn text_that_ends_too_soon_is_refused_at_its_end() {
    assert_eq!(syntax(""), (SyntaxProblem::EndOfText, 1, 1));
    assert_eq!(syntax(" \n "), (SyntaxProblem::EndOfText, 2, 2));
    assert_eq!(syntax(r#"{"version":1"#), (SyntaxProblem::EndOfText, 1, 13));
    assert_eq!(syntax(r#"{"version""#), (SyntaxProblem::EndOfText, 1, 11));
    assert_eq!(syntax(r#"{"a":"abc"#), (SyntaxProblem::EndOfText, 1, 10));
    assert_eq!(syntax(r#"{"a":tru"#), (SyntaxProblem::EndOfText, 1, 9));
    assert_eq!(syntax("[1,\n"), (SyntaxProblem::EndOfText, 2, 1));
}

#[test]
fn a_repeated_key_is_refused_at_the_second_one() {
    assert_eq!(
        syntax(r#"{"version":1,"steps":[],"version":1}"#),
        (SyntaxProblem::DuplicateKey("version".to_owned()), 1, 25)
    );
    // Keys are equal by their characters, however they are written.
    assert_eq!(
        syntax(r#"{"a":{"b":1,"\u0062":2}}"#),
        (SyntaxProblem::DuplicateKey("b".to_owned()), 1, 13)
    );
    // The same key in two objects is no repeat.
    assert!(matches!(
        Document::read(r#"{"a":{"k":1},"b":{"k":2}}"#),
        Err(ReadError::Refused(_))
    ));
}

#[test]
fn what_is_not_json_is_refused() {
    for (text, found, line, column) in [
        ("{'a':1}", '\'', 1, 2),
        ("{\"a\":1 // note\n}", '/', 1, 8),
        ("{/* */}", '/', 1, 2),
        ("\u{feff}{}", '\u{feff}', 1, 1),
        ("{\"a\":\u{b}1}", '\u{b}', 1, 6),
        ("{\"a\":\u{a0}1}", '\u{a0}', 1, 6),
        ("{\"a\":1,\"b\"}", '}', 1, 11),
        ("{\"a\" 1}", '1', 1, 6),
        ("{a:1}", 'a', 1, 2),
        ("{,}", ',', 1, 2),
        ("[,1]", ',', 1, 2),
        ("[1 2]", '2', 1, 4),
        ("{\"a\":True}", 'T', 1, 6),
        ("{\"a\":nulL}", 'L', 1, 9),
    ] {
        assert_eq!(
            syntax(text),
            (SyntaxProblem::Unexpected(found), line, column),
            "{text:?}"
        );
    }
}

#[test]
fn the_place_counts_lines_by_newline_and_columns_by_character() {
    // CRLF is one line break; the carriage return ends the line before it.
    assert_eq!(
        syntax("{\r\n\"a\":x}"),
        (SyntaxProblem::Unexpected('x'), 2, 5)
    );
    // Each of é and 😀 is one column, though more than one byte.
    assert_eq!(
        syntax("{\"é😀\":x}"),
        (SyntaxProblem::Unexpected('x'), 1, 7)
    );
    assert_eq!(syntax("\n\n\n  ]"), (SyntaxProblem::Unexpected(']'), 4, 3));
}

#[test]
fn whitespace_of_json_is_skipped_everywhere() {
    let text = " \t\r\n{ \"version\" \n:\t1 ,\r\"steps\" : [ ] \n}\n ";
    let document = Document::read(text).expect("a document");
    assert!(document.steps().is_empty());
}

#[test]
fn a_syntax_error_says_where_in_words() {
    let Err(error) = Document::read("{\n  \"version\": 1,\n  \"steps\": [1,]\n}") else {
        panic!("an error");
    };
    let words = error.to_string();
    assert!(words.contains("line 3, column 15"), "{words}");
}

#[test]
fn json_with_the_wrong_meaning_is_a_refusal_not_a_syntax_error() {
    // All of these are JSON by RFC 8259, whatever the document makes of them.
    for text in [
        r#"{"version":-0,"steps":[]}"#,
        r#"{"version":1E0,"steps":[]}"#,
        r#"{"version":1.5e-3,"steps":[]}"#,
        r#"{"version":null,"steps":[]}"#,
        r#"{"version":true,"steps":[]}"#,
        r#"[]"#,
        r#""text""#,
        r#"0"#,
    ] {
        assert!(
            matches!(Document::read(text), Err(ReadError::Refused(_))),
            "{text}"
        );
    }
}
