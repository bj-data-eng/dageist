use crate::{
    ArgumentKind, Comment, Expr, ExprKind, Link, LiteralValue, Position, Span, SyntaxProblem,
    parse, print,
};

fn refused(text: &str) -> (SyntaxProblem, (usize, usize), (usize, usize)) {
    let error = parse(text).expect_err("the text is refused");
    let span = error.span();
    (
        error.problem().clone(),
        (span.start().line(), span.start().column()),
        (span.end().line(), span.end().column()),
    )
}

fn chain(text: &str) -> crate::Chain {
    match parse(text).expect("the text reads").expression.kind {
        ExprKind::Chain(chain) => chain,
        other => panic!("not a chain: {other:?}"),
    }
}

fn literal(text: &str) -> LiteralValue {
    match parse(text).expect("the text reads").expression.kind {
        ExprKind::Literal(literal) => literal.value,
        other => panic!("not a literal: {other:?}"),
    }
}

#[test]
fn a_chain_reads_as_its_head_then_attributes_and_calls() {
    let read = chain("pl.col(\"a\").str.contains(r\"\\d+\", literal=True)");
    assert_eq!(read.head.kind, ExprKind::Name("pl".to_owned()));
    let names: Vec<String> = read
        .links
        .iter()
        .map(|link| match link {
            Link::Attribute { name, .. } => format!(".{name}"),
            Link::Call { arguments, .. } => format!("({})", arguments.len()),
        })
        .collect();
    assert_eq!(names, [".col", "(1)", ".str", ".contains", "(2)"]);
    let Link::Call { arguments, .. } = &read.links[4] else {
        panic!("a call");
    };
    let ArgumentKind::Positional(pattern) = &arguments[0].kind else {
        panic!("by position");
    };
    let ExprKind::Literal(pattern) = &pattern.kind else {
        panic!("a literal");
    };
    assert_eq!(pattern.value, LiteralValue::String("\\d+".to_owned()));
    assert_eq!(pattern.written, "r\"\\d+\"");
    let ArgumentKind::Keyword { name, value, .. } = &arguments[1].kind else {
        panic!("by name");
    };
    assert_eq!(name, "literal");
    assert_eq!(
        value.kind,
        ExprKind::Literal(crate::Literal {
            written: "True".to_owned(),
            value: LiteralValue::True,
        })
    );
}

#[test]
fn each_node_names_its_line_and_columns() {
    let read = chain("lf.sort(\n    \"a\",\n)");
    let Link::Attribute { dot, name_span, .. } = &read.links[0] else {
        panic!("an attribute");
    };
    assert_eq!(
        *dot,
        Span::new(Position::new(1, 3, 2), Position::new(1, 4, 3))
    );
    assert_eq!(name_span.start().column(), 4);
    assert_eq!(name_span.end().column(), 8);
    let Link::Call { span, arguments } = &read.links[1] else {
        panic!("a call");
    };
    assert_eq!((span.start().line(), span.start().column()), (1, 8));
    assert_eq!((span.end().line(), span.end().column()), (3, 2));
    assert_eq!(
        (
            arguments[0].span.start().line(),
            arguments[0].span.start().column()
        ),
        (2, 5)
    );
}

#[test]
fn numbers_read_as_python_reads_them() {
    assert_eq!(literal("1_000"), LiteralValue::Integer("1000".to_owned()));
    assert_eq!(literal("-7"), LiteralValue::Integer("-7".to_owned()));
    assert_eq!(literal("- 7"), LiteralValue::Integer("-7".to_owned()));
    assert_eq!(literal("0"), LiteralValue::Integer("0".to_owned()));
    assert_eq!(literal("2.5"), LiteralValue::Float("2.5".to_owned()));
    assert_eq!(literal(".5"), LiteralValue::Float("0.5".to_owned()));
    assert_eq!(literal("5."), LiteralValue::Float("5.0".to_owned()));
    assert_eq!(literal("1e+16"), LiteralValue::Float("1e+16".to_owned()));
    assert_eq!(
        literal("-1.5e-05"),
        LiteralValue::Float("-1.5e-05".to_owned())
    );
    for written in ["0x1f", "01", "1j", "1__0", "1_", "1e", "1.2.3"] {
        assert_eq!(
            refused(written).0,
            SyntaxProblem::BadNumber(written.to_owned()),
            "{written}"
        );
    }
}

#[test]
fn strings_read_their_escapes() {
    assert_eq!(
        literal(r#""a\"b\\c\n\t\x41\u00e9\U0001F600\101""#),
        LiteralValue::String("a\"b\\c\n\tA\u{e9}\u{1F600}A".to_owned())
    );
    assert_eq!(literal("'it'"), LiteralValue::String("it".to_owned()));
    assert_eq!(
        literal(r"r'\d\''"),
        LiteralValue::String(r"\d\'".to_owned())
    );
    assert_eq!(literal("u'x'"), LiteralValue::String("x".to_owned()));
    assert_eq!(literal("\"é\""), LiteralValue::String("é".to_owned()));
}

#[test]
fn a_string_python_does_not_hold_as_one_line_is_refused() {
    assert_eq!(
        refused("\"abc"),
        (SyntaxProblem::UnterminatedString, (1, 1), (1, 5))
    );
    assert_eq!(refused("'a\nb'").0, SyntaxProblem::UnterminatedString);
    assert_eq!(refused("\"\"\"doc\"\"\"").0, SyntaxProblem::TripleQuoted);
    assert_eq!(
        refused("f\"{x}\"").0,
        SyntaxProblem::StringPrefix("f".to_owned())
    );
    assert_eq!(
        refused("b'x'").0,
        SyntaxProblem::StringPrefix("b".to_owned())
    );
    assert_eq!(
        refused(r#""a\qb""#),
        (SyntaxProblem::BadEscape(r"\q".to_owned()), (1, 3), (1, 5))
    );
    assert_eq!(
        refused(r#""\x4""#).0,
        SyntaxProblem::BadEscape(r"\x4".to_owned())
    );
    assert_eq!(
        refused(r#""\ud800""#).0,
        SyntaxProblem::BadEscape(r"\ud800".to_owned())
    );
}

#[test]
fn python_beyond_the_notation_is_refused_by_what_it_is() {
    let cases: [(&str, SyntaxProblem, (usize, usize)); 16] = [
        ("x = lf", SyntaxProblem::Assignment, (1, 3)),
        ("x += 1", SyntaxProblem::Assignment, (1, 3)),
        ("(x := 1)", SyntaxProblem::Assignment, (1, 4)),
        (
            "lf.filter(lambda row: row)",
            SyntaxProblem::Keyword("lambda".to_owned()),
            (1, 11),
        ),
        (
            "import polars",
            SyntaxProblem::Keyword("import".to_owned()),
            (1, 1),
        ),
        (
            "for x in y",
            SyntaxProblem::Keyword("for".to_owned()),
            (1, 1),
        ),
        ("[x for x in y]", SyntaxProblem::Comprehension, (1, 4)),
        ("f(x for x in y)", SyntaxProblem::Comprehension, (1, 5)),
        (
            "pl.col(\"a\") + 1",
            SyntaxProblem::Operator("+".to_owned()),
            (1, 13),
        ),
        (
            "pl.col(\"a\") * 2",
            SyntaxProblem::Operator("*".to_owned()),
            (1, 13),
        ),
        (
            "~pl.col(\"a\")",
            SyntaxProblem::Operator("~".to_owned()),
            (1, 1),
        ),
        (
            "-pl.col(\"a\")",
            SyntaxProblem::Operator("-".to_owned()),
            (1, 1),
        ),
        (
            "x if y else z",
            SyntaxProblem::Keyword("if".to_owned()),
            (1, 3),
        ),
        ("lf[0]", SyntaxProblem::Subscript, (1, 3)),
        ("lf\nlf", SyntaxProblem::MoreThanOneStatement, (2, 1)),
        ("lf; lf", SyntaxProblem::MoreThanOneStatement, (1, 3)),
    ];
    for (text, problem, start) in cases {
        let (found, at, _) = refused(text);
        assert_eq!((found, at), (problem, start), "{text}");
    }
}

#[test]
fn a_malformed_text_is_refused_where_it_goes_wrong() {
    assert_eq!(
        refused("   \n# only a comment\n"),
        (SyntaxProblem::Empty, (3, 1), (3, 1))
    );
    assert_eq!(
        refused("f(1"),
        (SyntaxProblem::Unclosed('('), (1, 2), (1, 3))
    );
    assert_eq!(
        refused("[1, 2"),
        (SyntaxProblem::Unclosed('['), (1, 1), (1, 2))
    );
    assert_eq!(
        refused("f(a=1, 2)"),
        (SyntaxProblem::PositionalAfterKeyword, (1, 8), (1, 9))
    );
    assert_eq!(
        refused("f(**a, *b)").0,
        SyntaxProblem::PositionalAfterKeyword
    );
    assert!(
        parse("f(a=1, *b)").is_ok(),
        "Python reads a `*` after an argument by name"
    );
    assert_eq!(
        refused("f(a=1, b=2, a=3)"),
        (
            SyntaxProblem::RepeatedKeyword("a".to_owned()),
            (1, 13),
            (1, 14)
        )
    );
    assert_eq!(
        refused("a $ b"),
        (SyntaxProblem::UnexpectedCharacter('$'), (1, 3), (1, 4))
    );
    assert_eq!(refused("naïve").0, SyntaxProblem::UnexpectedCharacter('ï'));
    assert_eq!(
        refused("(1, 2)").0,
        SyntaxProblem::Unexpected {
            found: "`,`".to_owned(),
            expected: "`)`, as a tuple is not read,",
        }
    );
    assert_eq!(
        refused("{1, 2}").0,
        SyntaxProblem::Unexpected {
            found: "`,`".to_owned(),
            expected: "`:` after a key",
        }
    );
    assert_eq!(
        refused("f(1 2)").0,
        SyntaxProblem::Unexpected {
            found: "the literal `2`".to_owned(),
            expected: "`,` or `)`",
        }
    );
    assert_eq!(
        refused("a.1").0,
        SyntaxProblem::Unexpected {
            found: "the literal `.1`".to_owned(),
            expected: "the end of the line",
        }
    );
}

#[test]
fn brackets_nested_past_the_limit_are_refused_and_up_to_it_read() {
    let depth = crate::MAX_DEPTH;
    let within = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
    assert!(parse(&within).is_ok());
    let past = format!("{}1{}", "[".repeat(depth + 1), "]".repeat(depth + 1));
    let (problem, start, _) = refused(&past);
    assert_eq!((problem, start), (SyntaxProblem::TooDeep, (1, depth + 1)));
    let calls = format!("{}{}", "f(".repeat(depth + 1), ")".repeat(depth + 1));
    assert_eq!(refused(&calls).0, SyntaxProblem::TooDeep);
}

#[test]
fn a_long_chain_reads_without_nesting_the_tree() {
    let text = format!("x{}", ".abs()".repeat(50_000));
    assert_eq!(chain(&text).links.len(), 100_000);
}

#[test]
fn lines_inside_brackets_or_after_a_backslash_do_not_end_the_expression() {
    let read = chain("(\n    lf\n    .sort(\"a\")\n    .head(\n        5,\n    )\n)\n\n");
    assert_eq!(read.links.len(), 4);
    assert_eq!(chain("lf \\\n.sort()").links.len(), 2);
}

#[test]
fn a_comment_before_a_dot_is_the_attributes_and_any_other_is_loose() {
    let source =
        parse("# about\n(\n    lf  # the frame\n    # step: first\n    .sort(\"a\")  # sorted\n)")
            .expect("reads");
    let ExprKind::Chain(read) = source.expression.kind else {
        panic!("a chain");
    };
    let Link::Attribute { comments, .. } = &read.links[0] else {
        panic!("an attribute");
    };
    let texts: Vec<&str> = comments
        .iter()
        .map(|comment| comment.text.as_str())
        .collect();
    assert_eq!(texts, [" the frame", " step: first"]);
    let loose: Vec<&str> = source
        .comments
        .iter()
        .map(|comment| comment.text.as_str())
        .collect();
    assert_eq!(loose, [" about", " sorted"]);
    assert_eq!(source.comments[0].span.start(), Position::new(1, 1, 0));
}

#[test]
fn a_short_expression_prints_on_one_line() {
    let source = parse("lf.sort( 'a' ,descending = True )").expect("reads");
    assert_eq!(print(&source.expression), "lf.sort('a', descending=True)\n");
}

#[test]
fn a_chain_of_two_calls_prints_a_call_to_a_line_in_brackets() {
    let source = parse("lf.sort(\"a\").head(5)").expect("reads");
    assert_eq!(
        print(&source.expression),
        "(\n    lf\n    .sort(\"a\")\n    .head(5)\n)\n"
    );
}

#[test]
fn a_call_too_long_for_a_line_prints_an_argument_to_a_line() {
    let text = "lf.with_columns(pl.col(\"ordered_at\").shift(1).over(pl.col(\"customer_id\")).alias(\"previous_order\"), pl.col(\"amount\").fill_null(0))";
    let source = parse(text).expect("reads");
    assert_eq!(
        print(&source.expression),
        "lf.with_columns(\n    pl.col(\"ordered_at\").shift(1).over(pl.col(\"customer_id\")).alias(\"previous_order\"),\n    pl.col(\"amount\").fill_null(0),\n)\n"
    );
}

#[test]
fn a_nested_chain_too_long_for_its_line_prints_a_call_to_a_line() {
    let text = "lf.with_columns(pl.col(\"ordered_at_with_a_long_name\").shift(1).over(pl.col(\"customer_id_with_a_long_name\")).alias(\"previous_order\"))";
    let source = parse(text).expect("reads");
    assert_eq!(
        print(&source.expression),
        "lf.with_columns(\n    pl.col(\"ordered_at_with_a_long_name\")\n    .shift(1)\n    .over(pl.col(\"customer_id_with_a_long_name\"))\n    .alias(\"previous_order\"),\n)\n"
    );
}

#[test]
fn an_attributes_comment_prints_on_its_own_line_before_the_dot() {
    let made = Expr::chain(
        Expr::name("lf"),
        vec![
            Link::Attribute {
                dot: Span::default(),
                name: "sort".to_owned(),
                name_span: Span::default(),
                comments: vec![Comment::new(" step: sort_2")],
            },
            Link::call(Vec::new()),
        ],
    );
    let text = print(&made);
    assert_eq!(text, "(\n    lf\n    # step: sort_2\n    .sort()\n)\n");
    let again = parse(&text).expect("reads");
    assert_eq!(print(&again.expression), text);
}

#[test]
fn a_number_at_the_head_of_a_chain_prints_in_brackets() {
    let source = parse("(1).abs()").expect("reads");
    let text = print(&source.expression);
    assert_eq!(text, "(1).abs()\n");
    assert_eq!(print(&parse(&text).expect("reads").expression), text);
    let source = parse("(-2.5).abs()").expect("reads");
    assert_eq!(print(&source.expression), "(-2.5).abs()\n");
}

#[test]
fn an_ellipsis_followed_by_a_method_prints_as_it_reads() {
    let source = parse("....abs()").expect("reads");
    let text = print(&source.expression);
    assert_eq!(text, "....abs()\n");
    let ExprKind::Chain(read) = parse(&text).expect("reads").expression.kind else {
        panic!("a chain");
    };
    assert_eq!(read.head.kind, ExprKind::Ellipsis);
}

#[test]
fn an_outermost_chain_broken_after_one_call_prints_in_brackets() {
    let long = "x".repeat(80);
    let source = parse(&format!("lf.f({long:?}).y")).expect("reads");
    let text = print(&source.expression);
    assert_eq!(
        text,
        format!("(\n    lf\n    .f(\n        {long:?},\n    )\n    .y\n)\n")
    );
    assert_eq!(print(&parse(&text).expect("reads").expression), text);
}
