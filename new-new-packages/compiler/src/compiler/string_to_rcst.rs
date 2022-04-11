use super::rcst::{Rcst, RcstError};
use crate::input::{Input, InputDb};
use std::sync::Arc;

#[salsa::query_group(StringToRcstStorage)]
pub trait StringToRcst: InputDb {
    fn rcst(&self, input: Input) -> Option<Arc<Vec<Rcst>>>;
}

fn rcst(db: &dyn StringToRcst, input: Input) -> Option<Arc<Vec<Rcst>>> {
    let source = db.get_input(input)?;
    let (rest, mut rcsts) = parse::body(&source, 0);
    if !rest.is_empty() {
        rcsts.push(Rcst::Error {
            unparsable_input: rest.to_string(),
            error: RcstError::UnparsedRest,
        });
    }
    Some(Arc::new(rcsts))
}

impl Rcst {
    fn wrap_in_whitespace(mut self, mut whitespace: Vec<Rcst>) -> Self {
        if !whitespace.is_empty() {
            if let Rcst::TrailingWhitespace {
                whitespace: self_whitespace,
                ..
            } = &mut self
            {
                self_whitespace.append(&mut whitespace);
                self
            } else {
                Rcst::TrailingWhitespace {
                    child: Box::new(self),
                    whitespace,
                }
            }
        } else {
            self
        }
    }
}

mod parse {
    // All parsers take an input and return an input that may have advanced a
    // little.
    //
    // Note: The parser is indentation-first. Indentation is more important than
    // parentheses, brackets, etc. If some part of a definition can't be parsed,
    // all the surrounding code still has a chance to be properly parsed – even
    // mid-writing after putting the opening bracket of a struct.

    use super::super::rcst::{IsMultiline, Rcst, RcstError};
    use itertools::Itertools;

    static MEANINGFUL_PUNCTUATION: &'static str = "=:,(){}[]->";

    fn literal<'a>(input: &'a str, literal: &'static str) -> Option<&'a str> {
        log::trace!("literal({:?}, {:?})", input, literal);
        if input.starts_with(literal) {
            Some(&input[literal.len()..])
        } else {
            None
        }
    }
    #[test]
    fn test_literal() {
        assert_eq!(literal("hello, world", "hello"), Some(", world"));
        assert_eq!(literal("hello, world", "hi"), None);
    }

    pub fn equals_sign(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "=")?;
        Some((input, Rcst::EqualsSign))
    }
    pub fn comma(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, ",")?;
        Some((input, Rcst::Comma))
    }
    pub fn colon(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, ":")?;
        Some((input, Rcst::Colon))
    }
    fn opening_bracket(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "[")?;
        Some((input, Rcst::OpeningBracket))
    }
    pub fn closing_bracket(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "]")?;
        Some((input, Rcst::ClosingBracket))
    }
    fn opening_parenthesis(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "(")?;
        Some((input, Rcst::OpeningParenthesis))
    }
    pub fn closing_parenthesis(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, ")")?;
        Some((input, Rcst::ClosingParenthesis))
    }
    fn opening_curly_brace(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "{")?;
        Some((input, Rcst::OpeningCurlyBrace))
    }
    pub fn closing_curly_brace(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "}")?;
        Some((input, Rcst::ClosingCurlyBrace))
    }
    pub fn arrow(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "->")?;
        Some((input, Rcst::Arrow))
    }
    fn double_quote(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "\"")?;
        Some((input, Rcst::DoubleQuote))
    }
    fn octothorpe(input: &str) -> Option<(&str, Rcst)> {
        let input = literal(input, "#")?;
        Some((input, Rcst::Octothorpe))
    }

    /// "Word" refers to a number of characters that are not separated by
    /// whitespace or significant punctuation. Identifiers, symbols, and ints
    /// are words. Words may be invalid because they contain non-ascii or
    /// non-alphanumeric characters – for example, the word `Magic✨` is an
    /// invalid identifier or symbol.
    fn word(mut input: &str) -> Option<(&str, String)> {
        log::trace!("word({:?})", input);
        let mut chars = vec![];
        while let Some(c) = input.chars().next() {
            if c.is_whitespace() || MEANINGFUL_PUNCTUATION.contains(c) {
                break;
            }
            chars.push(c);
            input = &input[c.len_utf8()..];
        }
        if chars.is_empty() {
            None
        } else {
            Some((input, chars.into_iter().join("")))
        }
    }
    #[test]
    fn test_word() {
        assert_eq!(word("hello, world"), Some((", world", "hello".to_string())));
        assert_eq!(
            word("I💖Candy blub"),
            Some((" blub", "I💖Candy".to_string()))
        );
        assert_eq!(word("012🔥hi"), Some(("", "012🔥hi".to_string())));
        assert_eq!(word("foo(blub)"), Some(("(blub)", "foo".to_string())));
    }

    fn identifier(input: &str) -> Option<(&str, Rcst)> {
        log::trace!("identifier({:?})", input);
        let (input, w) = word(input)?;
        if w.chars().next().unwrap().is_lowercase() {
            if w.chars().all(|c| c.is_ascii_alphanumeric()) {
                Some((input, Rcst::Identifier(w)))
            } else {
                Some((
                    input,
                    Rcst::Error {
                        unparsable_input: w,
                        error: RcstError::IdentifierContainsNonAlphanumericAscii,
                    },
                ))
            }
        } else {
            None
        }
    }
    #[test]
    fn test_identifier() {
        assert_eq!(
            identifier("foo bar"),
            Some((" bar", Rcst::Identifier("foo".to_string())))
        );
        assert_eq!(identifier("Foo bar"), None);
        assert_eq!(identifier("012 bar"), None);
        assert_eq!(
            identifier("f12🔥 bar"),
            Some((
                " bar",
                Rcst::Error {
                    unparsable_input: "f12🔥".to_string(),
                    error: RcstError::IdentifierContainsNonAlphanumericAscii,
                }
            ))
        );
    }

    fn symbol(input: &str) -> Option<(&str, Rcst)> {
        log::trace!("symbol({:?})", input);
        let (input, w) = word(input)?;
        if w.chars().next().unwrap().is_uppercase() {
            if w.chars().all(|c| c.is_ascii_alphanumeric()) {
                Some((input, Rcst::Symbol(w)))
            } else {
                Some((
                    input,
                    Rcst::Error {
                        unparsable_input: w,
                        error: RcstError::SymbolContainsNonAlphanumericAscii,
                    },
                ))
            }
        } else {
            None
        }
    }
    #[test]
    fn test_symbol() {
        assert_eq!(
            symbol("Foo b"),
            Some((" b", Rcst::Symbol("Foo".to_string())))
        );
        assert_eq!(symbol("foo bar"), None);
        assert_eq!(symbol("012 bar"), None);
        assert_eq!(
            symbol("F12🔥 bar"),
            Some((
                " bar",
                Rcst::Error {
                    unparsable_input: "F12🔥".to_string(),
                    error: RcstError::SymbolContainsNonAlphanumericAscii,
                }
            ))
        );
    }

    fn int(input: &str) -> Option<(&str, Rcst)> {
        log::trace!("int({:?})", input);
        let (input, w) = word(input)?;
        if w.chars().next().unwrap().is_ascii_digit() {
            if w.chars().all(|c| c.is_ascii_digit()) {
                let value = u64::from_str_radix(&w, 10).expect("Couldn't parse int.");
                Some((input, Rcst::Int(value)))
            } else {
                Some((
                    input,
                    Rcst::Error {
                        unparsable_input: w,
                        error: RcstError::IntContainsNonDigits,
                    },
                ))
            }
        } else {
            None
        }
    }
    #[test]
    fn test_int() {
        assert_eq!(int("42 "), Some((" ", Rcst::Int(42))));
        assert_eq!(int("123 years"), Some((" years", Rcst::Int(123))));
        assert_eq!(int("foo"), None);
        assert_eq!(
            int("3D"),
            Some((
                "",
                Rcst::Error {
                    unparsable_input: "3D".to_string(),
                    error: RcstError::IntContainsNonDigits,
                }
            ))
        );
    }

    fn single_line_whitespace(mut input: &str) -> (&str, Rcst) {
        log::trace!("single_line_whitespace({:?})", input);
        let mut chars = vec![];
        let mut has_error = false;
        while let Some(c) = input.chars().next() {
            match c {
                ' ' => {
                    chars.push(' ');
                    input = &input[1..];
                }
                c if c.is_whitespace() && c != '\n' => {
                    chars.push(c);
                    has_error = true;
                    input = &input[c.len_utf8()..];
                }
                _ => break,
            }
        }
        let whitespace = chars.into_iter().join("");
        if has_error {
            (
                input,
                Rcst::Error {
                    unparsable_input: whitespace,
                    error: RcstError::WeirdWhitespace,
                },
            )
        } else {
            (input, Rcst::Whitespace(whitespace))
        }
    }

    fn comment(input: &str) -> Option<(&str, Rcst)> {
        log::trace!("comment({:?})", input);
        let (mut input, octothorpe) = octothorpe(input)?;
        let mut comment = vec![];
        loop {
            match input.chars().next() {
                Some('\n') | None => {
                    break;
                }
                Some(c) => {
                    comment.push(c);
                    input = &input[c.len_utf8()..];
                }
            }
        }
        Some((
            input,
            Rcst::Comment {
                octothorpe: Box::new(octothorpe),
                comment: comment.into_iter().join(""),
            },
        ))
    }

    fn leading_indentation(mut input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("leading_indentation({:?}, {:?})", input, indentation);
        let mut chars = vec![];
        let mut has_weird_whitespace = false;
        let mut indent_in_spaces = 0;

        while indent_in_spaces < 2 * indentation {
            let c = input.chars().next()?;
            let (is_weird, indent_bonus) = match c {
                ' ' => (false, 1),
                '\t' => (true, 2),
                c if c.is_whitespace() => (true, 1),
                _ => return None,
            };
            chars.push(c);
            has_weird_whitespace |= is_weird;
            indent_in_spaces += indent_bonus;
            input = &input[c.len_utf8()..];
        }
        let whitespace = chars.into_iter().join("");
        Some(if has_weird_whitespace {
            (
                input,
                Rcst::Error {
                    unparsable_input: whitespace,
                    error: RcstError::WeirdWhitespaceInIndentation,
                },
            )
        } else {
            (input, Rcst::Whitespace(whitespace))
        })
    }
    #[test]
    fn test_leading_indentation() {
        assert_eq!(
            leading_indentation("foo", 0),
            Some(("foo", Rcst::Whitespace("".to_string())))
        );
        assert_eq!(
            leading_indentation("  foo", 1),
            Some(("foo", Rcst::Whitespace("  ".to_string())))
        );
        assert_eq!(leading_indentation("  foo", 2), None);
    }

    /// Consumes all leading whitespace (including newlines) and comments that
    /// are still within the given indentation. Won't consume newlines before a
    /// lower or higher indentation.
    pub fn whitespaces_and_newlines(
        input: &str,
        indentation: usize,
        also_comments: bool,
    ) -> (&str, Vec<Rcst>) {
        log::trace!(
            "whitespaces_and_newlines({:?}, {:?}, {:?})",
            input,
            indentation,
            also_comments
        );
        let mut parts = vec![];
        let (input, whitespace) = single_line_whitespace(input);
        parts.push(whitespace);

        let mut input = input;
        loop {
            if also_comments {
                if let Some((i, whitespace)) = comment(input) {
                    input = i;
                    parts.push(whitespace);
                }
            }

            // We only consume newlines if there is sufficient indentation
            // coming after.
            let mut new_input = input;
            let mut new_parts = vec![];
            while let Some('\n') = new_input.chars().next() {
                new_parts.push(Rcst::Newline);
                new_input = &new_input[1..];
            }
            if new_input == input {
                break; // No newlines.
            }
            match leading_indentation(new_input, indentation) {
                Some((new_input, whitespace)) => {
                    new_parts.push(Rcst::Whitespace(whitespace.to_string()));
                    parts.append(&mut new_parts);
                    input = new_input;
                }
                None => break,
            }
        }
        let parts = parts
            .into_iter()
            .filter(|it| {
                if let Rcst::Whitespace(ws) = it {
                    !ws.is_empty()
                } else {
                    true
                }
            })
            .collect();
        (input, parts)
    }
    #[test]
    fn test_whitespaces_and_newlines() {
        assert_eq!(whitespaces_and_newlines("foo", 0, true), ("foo", vec![]));
        assert_eq!(
            whitespaces_and_newlines("\nfoo", 0, true),
            ("foo", vec![Rcst::Newline])
        );
        assert_eq!(
            whitespaces_and_newlines("\n  foo", 1, true),
            (
                "foo",
                vec![Rcst::Newline, Rcst::Whitespace("  ".to_string())]
            )
        );
        assert_eq!(
            whitespaces_and_newlines("\n  foo", 0, true),
            ("  foo", vec![Rcst::Newline])
        );
        assert_eq!(
            whitespaces_and_newlines(" \n  foo", 0, true),
            (
                "  foo",
                vec![Rcst::Whitespace(" ".to_string()), Rcst::Newline]
            )
        );
        assert_eq!(
            whitespaces_and_newlines("\n  foo", 2, true),
            ("\n  foo", vec![])
        );
        assert_eq!(
            whitespaces_and_newlines("\tfoo", 1, true),
            (
                "foo",
                vec![Rcst::Error {
                    unparsable_input: "\t".to_string(),
                    error: RcstError::WeirdWhitespace
                }]
            )
        );
        assert_eq!(
            whitespaces_and_newlines("# hey\n  foo", 1, true),
            (
                "foo",
                vec![
                    Rcst::Comment {
                        octothorpe: Box::new(Rcst::Octothorpe),
                        comment: " hey".to_string()
                    },
                    Rcst::Newline,
                    Rcst::Whitespace("  ".to_string()),
                ],
            )
        );
    }

    fn text(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("text({:?}, {:?})", input, indentation);
        let (mut input, opening_quote) = double_quote(input)?;
        let mut line = vec![];
        let mut parts = vec![];
        let closing_quote = loop {
            match input.chars().next() {
                Some('"') => {
                    input = &input[1..];
                    parts.push(Rcst::TextPart(line.drain(..).join("")));
                    break Rcst::DoubleQuote;
                }
                None => {
                    parts.push(Rcst::TextPart(line.drain(..).join("")));
                    break Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::TextDoesNotEndUntilInputEnds,
                    };
                }
                Some('\n') => {
                    parts.push(Rcst::TextPart(line.drain(..).join("")));
                    let (i, mut whitespace) =
                        whitespaces_and_newlines(input, indentation + 1, false);
                    input = i;
                    parts.append(&mut whitespace);
                    if let Some('\n') = input.chars().next() {
                        break Rcst::Error {
                            unparsable_input: "".to_string(),
                            error: RcstError::TextNotSufficientlyIndented,
                        };
                    }
                }
                Some(c) => {
                    input = &input[c.len_utf8()..];
                    line.push(c);
                }
            }
        };
        Some((
            input,
            Rcst::Text {
                opening_quote: Box::new(opening_quote),
                parts,
                closing_quote: Box::new(closing_quote),
            },
        ))
    }
    #[test]
    fn test_text() {
        assert_eq!(text("foo", 0), None);
        assert_eq!(
            text("\"foo\" bar", 0),
            Some((
                " bar",
                Rcst::Text {
                    opening_quote: Box::new(Rcst::DoubleQuote),
                    parts: vec![Rcst::TextPart("foo".to_string())],
                    closing_quote: Box::new(Rcst::DoubleQuote)
                }
            ))
        );
        // "foo
        //   bar"2
        assert_eq!(
            text("\"foo\n  bar\"2", 0),
            Some((
                "2",
                Rcst::Text {
                    opening_quote: Box::new(Rcst::DoubleQuote),
                    parts: vec![
                        Rcst::TextPart("foo".to_string()),
                        Rcst::Newline,
                        Rcst::Whitespace("  ".to_string()),
                        Rcst::TextPart("bar".to_string())
                    ],
                    closing_quote: Box::new(Rcst::DoubleQuote),
                }
            ))
        );
        //   "foo
        //   bar"
        assert_eq!(
            text("\"foo\n  bar\"2", 1),
            Some((
                "\n  bar\"2",
                Rcst::Text {
                    opening_quote: Box::new(Rcst::DoubleQuote),
                    parts: vec![Rcst::TextPart("foo".to_string()),],
                    closing_quote: Box::new(Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::TextNotSufficientlyIndented,
                    }),
                }
            ))
        );
        assert_eq!(
            text("\"foo", 0),
            Some((
                "",
                Rcst::Text {
                    opening_quote: Box::new(Rcst::DoubleQuote),
                    parts: vec![Rcst::TextPart("foo".to_string()),],
                    closing_quote: Box::new(Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::TextDoesNotEndUntilInputEnds,
                    }),
                }
            ))
        );
    }

    fn expression(
        input: &str,
        indentation: usize,
        allow_call_and_assignment: bool,
    ) -> Option<(&str, Rcst)> {
        log::trace!(
            "expression({:?}, {:?}, {:?})",
            input,
            indentation,
            allow_call_and_assignment
        );
        int(input)
            .or_else(|| text(input, indentation))
            .or_else(|| symbol(input))
            .or_else(|| struct_(input, indentation))
            .or_else(|| parenthesized(input, indentation))
            .or_else(|| lambda(input, indentation))
            .or_else(|| {
                if allow_call_and_assignment {
                    assignment(input, indentation)
                } else {
                    None
                }
            })
            .or_else(|| {
                if allow_call_and_assignment {
                    call(input, indentation)
                } else {
                    None
                }
            })
            .or_else(|| identifier(input))
            .or_else(|| {
                word(input).map(|(input, word)| {
                    (
                        input,
                        Rcst::Error {
                            unparsable_input: word,
                            error: RcstError::UnexpectedPunctuation,
                        },
                    )
                })
            })
    }
    #[test]
    fn test_expression() {
        assert_eq!(
            text("foo", 0),
            Some(("", Rcst::Identifier("foo".to_string())))
        );
    }

    /// Multiple expressions that are occurring one after another.
    fn run_of_expressions(input: &str, indentation: usize) -> Option<(&str, Vec<Rcst>)> {
        log::trace!("run_of_expressions({:?}, {:?})", input, indentation);
        let mut expressions = vec![];
        let (mut input, expr) = expression(input, indentation, false)?;
        expressions.push(expr);

        let mut has_multiline_whitespace = false;
        loop {
            let (i, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
            has_multiline_whitespace |= whitespace.is_multiline();
            let indentation = if has_multiline_whitespace {
                indentation + 1
            } else {
                indentation
            };

            let (i, expr) = match expression(i, indentation, has_multiline_whitespace) {
                Some(it) => it,
                None => {
                    let fallback = closing_parenthesis(i)
                        .or_else(|| closing_bracket(i))
                        .or_else(|| closing_curly_brace(i))
                        .or_else(|| arrow(i));
                    if let Some((i, cst)) = fallback {
                        (i, cst)
                    } else {
                        break;
                    }
                }
            };

            let last = expressions.pop().unwrap();
            expressions.push(last.wrap_in_whitespace(whitespace));

            expressions.push(expr);
            input = i;
        }
        Some((input, expressions))
    }

    fn call(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("call({:?}, {:?})", input, indentation);
        let (input, mut expressions) = run_of_expressions(input, indentation)?;
        if expressions.len() < 2 {
            return None;
        }
        let arguments = expressions.split_off(1);
        let name = expressions.into_iter().next().unwrap();
        Some((
            input,
            Rcst::Call {
                name: Box::new(name),
                arguments,
            },
        ))
    }
    #[test]
    fn test_call() {
        assert_eq!(call("print", 0), None);
        assert_eq!(
            call("foo bar", 0),
            Some((
                "",
                Rcst::Call {
                    name: Box::new(Rcst::Identifier("foo".to_string())),
                    arguments: vec![Rcst::Identifier("bar".to_string())]
                }
            ))
        );
        assert_eq!(
            call("Foo 4 bar", 0),
            Some((
                "",
                Rcst::Call {
                    name: Box::new(Rcst::Symbol("Foo".to_string())),
                    arguments: vec![Rcst::Int(4), Rcst::Identifier("bar".to_string())]
                }
            ))
        );
        // foo
        //   bar
        //   baz
        // 2
        assert_eq!(
            call("foo\n  bar\n  baz\n2", 0),
            Some((
                "\n2",
                Rcst::Call {
                    name: Box::new(Rcst::Identifier("foo".to_string())),
                    arguments: vec![
                        Rcst::Identifier("bar".to_string()),
                        Rcst::Identifier("baz".to_string())
                    ],
                },
            ))
        );
        // foo 1 2
        //   3
        //   4
        // bar
        assert_eq!(
            call("foo 1 2\n  3\n  4\nbar", 0),
            Some((
                "\nbar",
                Rcst::Call {
                    name: Box::new(Rcst::Identifier("foo".to_string())),
                    arguments: vec![Rcst::Int(1), Rcst::Int(2), Rcst::Int(3), Rcst::Int(4)],
                }
            ))
        );
    }

    fn struct_(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("struct({:?}, {:?})", input, indentation);

        let (mut outer_input, mut opening_bracket) = opening_bracket(input)?;

        let mut fields: Vec<Rcst> = vec![];
        let mut fields_indentation = indentation;
        loop {
            let input = outer_input;

            // Whitespace before key.
            let (input, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
            if whitespace.is_multiline() {
                fields_indentation = indentation + 1;
            }
            if fields.is_empty() {
                opening_bracket = opening_bracket.wrap_in_whitespace(whitespace);
            } else {
                let last = fields.pop().unwrap();
                fields.push(last.wrap_in_whitespace(whitespace));
            }

            // The key itself.
            let (input, key, has_key) = match expression(input, fields_indentation, true) {
                Some((input, key)) => (input, key, true),
                None => (
                    input,
                    Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::StructFieldMissesKey,
                    },
                    false,
                ),
            };

            // Whitespace between key and colon.
            let (input, whitespace) = whitespaces_and_newlines(input, fields_indentation + 1, true);
            if whitespace.is_multiline() {
                fields_indentation = indentation + 1;
            }
            let key = key.wrap_in_whitespace(whitespace);

            // Colon.
            let (input, colon, has_colon) = match colon(input) {
                Some((input, colon)) => (input, colon, true),
                None => (
                    input,
                    Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::StructFieldMissesColon,
                    },
                    false,
                ),
            };

            // Whitespace between colon and value.
            let (input, whitespace) = whitespaces_and_newlines(input, fields_indentation + 1, true);
            if whitespace.is_multiline() {
                fields_indentation = indentation + 1;
            }
            let colon = colon.wrap_in_whitespace(whitespace);

            // Value.
            let (input, value, has_value) = match expression(input, fields_indentation + 1, true) {
                Some((input, value)) => (input, value, true),
                None => (
                    input,
                    Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::StructFieldMissesValue,
                    },
                    false,
                ),
            };

            // Whitespace between value and comma.
            let (input, whitespace) = whitespaces_and_newlines(input, fields_indentation + 1, true);
            if whitespace.is_multiline() {
                fields_indentation = indentation + 1;
            }
            let value = value.wrap_in_whitespace(whitespace);

            // Comma.
            let (input, comma) = match comma(input) {
                Some((input, comma)) => (input, Some(comma)),
                None => (input, None),
            };

            if !has_key && !has_colon && !has_value && comma.is_none() {
                break;
            }

            outer_input = input;
            fields.push(Rcst::StructField {
                key: Box::new(key),
                colon: Box::new(colon),
                value: Box::new(value),
                comma: comma.map(|it| Box::new(it)),
            });
        }
        let input = outer_input;

        let (new_input, whitespace) = whitespaces_and_newlines(input, indentation, true);

        let (input, closing_bracket) = match closing_bracket(new_input) {
            Some((input, closing_bracket)) => {
                if fields.is_empty() {
                    opening_bracket = opening_bracket.wrap_in_whitespace(whitespace);
                } else {
                    let last = fields.pop().unwrap();
                    fields.push(last.wrap_in_whitespace(whitespace));
                }
                (input, closing_bracket)
            }
            None => (
                input,
                Rcst::Error {
                    unparsable_input: "".to_string(),
                    error: RcstError::StructNotClosed,
                },
            ),
        };

        Some((
            input,
            Rcst::Struct {
                opening_bracket: Box::new(opening_bracket),
                fields,
                closing_bracket: Box::new(closing_bracket),
            },
        ))
    }
    #[test]
    fn test_struct() {
        assert_eq!(struct_("hello", 0), None);
        assert_eq!(
            struct_("[]", 0),
            Some((
                "",
                Rcst::Struct {
                    opening_bracket: Box::new(Rcst::OpeningBracket),
                    fields: vec![],
                    closing_bracket: Box::new(Rcst::ClosingBracket),
                }
            ))
        );
        assert_eq!(
            struct_("[foo:bar]", 0),
            Some((
                "",
                Rcst::Struct {
                    opening_bracket: Box::new(Rcst::OpeningBracket),
                    fields: vec![Rcst::StructField {
                        key: Box::new(Rcst::Identifier("foo".to_string())),
                        colon: Box::new(Rcst::Colon),
                        value: Box::new(Rcst::Identifier("bar".to_string())),
                        comma: None,
                    },],
                    closing_bracket: Box::new(Rcst::ClosingBracket),
                }
            ))
        );
        // [
        //   foo: bar,
        //   4: "Hi",
        // ]
        assert_eq!(
            struct_("[\n  foo: bar,\n  4: \"Hi\",\n]", 0),
            Some((
                "",
                Rcst::Struct {
                    opening_bracket: Box::new(Rcst::TrailingWhitespace {
                        child: Box::new(Rcst::OpeningBracket),
                        whitespace: vec![Rcst::Newline, Rcst::Whitespace("  ".to_string())],
                    }),
                    fields: vec![
                        Rcst::TrailingWhitespace {
                            child: Box::new(Rcst::StructField {
                                key: Box::new(Rcst::Identifier("foo".to_string())),
                                colon: Box::new(Rcst::TrailingWhitespace {
                                    child: Box::new(Rcst::Colon),
                                    whitespace: vec![Rcst::Whitespace(" ".to_string())],
                                }),
                                value: Box::new(Rcst::Identifier("bar".to_string())),
                                comma: Some(Box::new(Rcst::Comma)),
                            }),
                            whitespace: vec![Rcst::Newline, Rcst::Whitespace("  ".to_string())]
                        },
                        Rcst::TrailingWhitespace {
                            child: Box::new(Rcst::StructField {
                                key: Box::new(Rcst::Int(4)),
                                colon: Box::new(Rcst::TrailingWhitespace {
                                    child: Box::new(Rcst::Colon),
                                    whitespace: vec![Rcst::Whitespace(" ".to_string())],
                                }),
                                value: Box::new(Rcst::Text {
                                    opening_quote: Box::new(Rcst::DoubleQuote),
                                    parts: vec![Rcst::TextPart("Hi".to_string())],
                                    closing_quote: Box::new(Rcst::DoubleQuote),
                                }),
                                comma: Some(Box::new(Rcst::Comma))
                            }),
                            whitespace: vec![Rcst::Newline]
                        }
                    ],
                    closing_bracket: Box::new(Rcst::ClosingBracket),
                }
            ))
        );
    }

    fn parenthesized(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("parenthesized({:?}, {:?})", input, indentation);

        let (input, opening_parenthesis) = opening_parenthesis(input)?;

        let (input, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
        let inner_indentation = if whitespace.is_multiline() {
            indentation + 1
        } else {
            indentation
        };
        let opening_parenthesis = opening_parenthesis.wrap_in_whitespace(whitespace);

        let (input, inner) = expression(input, inner_indentation, true).unwrap_or((
            input,
            Rcst::Error {
                unparsable_input: "".to_string(),
                error: RcstError::ExpressionExpectedAfterOpeningParenthesis,
            },
        ));

        let (input, whitespace) = whitespaces_and_newlines(input, indentation, true);
        let inner = inner.wrap_in_whitespace(whitespace);

        let (input, closing_parenthesis) = closing_parenthesis(input).unwrap_or((
            input,
            Rcst::Error {
                unparsable_input: "".to_string(),
                error: RcstError::ParenthesisNotClosed,
            },
        ));

        Some((
            input,
            Rcst::Parenthesized {
                opening_parenthesis: Box::new(opening_parenthesis),
                inner: Box::new(inner),
                closing_parenthesis: Box::new(closing_parenthesis),
            },
        ))
    }
    #[test]
    fn test_parenthesized() {
        assert_eq!(
            parenthesized("(foo)", 0),
            Some((
                "",
                Rcst::Parenthesized {
                    opening_parenthesis: Box::new(Rcst::OpeningParenthesis),
                    inner: Box::new(Rcst::Identifier("foo".to_string())),
                    closing_parenthesis: Box::new(Rcst::ClosingParenthesis),
                }
            ))
        );
        assert_eq!(parenthesized("foo", 0), None);
        assert_eq!(
            parenthesized("(foo", 0),
            Some((
                "",
                Rcst::Parenthesized {
                    opening_parenthesis: Box::new(Rcst::OpeningParenthesis),
                    inner: Box::new(Rcst::Identifier("foo".to_string())),
                    closing_parenthesis: Box::new(Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::ParenthesisNotClosed
                    }),
                }
            ))
        );
    }

    pub fn body(mut input: &str, indentation: usize) -> (&str, Vec<Rcst>) {
        log::trace!("body({:?}, {:?})", input, indentation);
        let mut expressions = vec![];
        loop {
            let mut new_expressions = vec![];
            let mut new_input = input;

            let (new_new_input, mut whitespace) =
                whitespaces_and_newlines(new_input, indentation, true);
            new_expressions.append(&mut whitespace);
            new_input = new_new_input;

            let (mut new_input, unexpected_whitespace) = single_line_whitespace(new_input);
            let mut indentation = indentation;
            if let Rcst::Whitespace(whitespace) = &unexpected_whitespace {
                if !whitespace.is_empty() {
                    indentation += whitespace.len() / 2; // TODO
                    new_expressions.push(Rcst::Error {
                        unparsable_input: whitespace.to_string(),
                        error: RcstError::TooMuchWhitespace,
                    });
                }
            } else {
                new_expressions.push(unexpected_whitespace);
            }

            match expression(new_input, indentation, true) {
                Some((new_new_input, expression)) => {
                    new_input = new_new_input;
                    new_expressions.push(expression);
                }
                None => {
                    let fallback = colon(new_input)
                        .or_else(|| comma(new_input))
                        .or_else(|| closing_parenthesis(new_input))
                        .or_else(|| closing_bracket(new_input))
                        .or_else(|| closing_curly_brace(new_input))
                        .or_else(|| arrow(new_input));
                    if let Some((i, cst)) = fallback {
                        new_input = i;
                        new_expressions.push(cst);
                    } else {
                        break (input, expressions);
                    }
                }
            }
            input = new_input;
            expressions.append(&mut new_expressions);
        }
    }

    fn lambda(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("lambda({:?}, {:?})", input, indentation);
        let (input, mut opening_curly_brace) = opening_curly_brace(input)?;
        let (mut input, mut parameters_and_arrow) = {
            let input_without_params = input;
            let mut input = input;
            let mut parameters = vec![];
            loop {
                let (i, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
                if parameters.is_empty() {
                    opening_curly_brace = opening_curly_brace.wrap_in_whitespace(whitespace);
                }

                input = i;
                match expression(input, indentation + 1, false) {
                    Some((i, parameter)) => {
                        input = i;
                        parameters.push(parameter);
                    }
                    None => break,
                };
            }
            match arrow(input) {
                Some((input, arrow)) => (input, Some((parameters, arrow))),
                None => (input_without_params, None),
            }
        };

        let (i, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
        if let Some((parameters, arrow)) = parameters_and_arrow {
            parameters_and_arrow = Some((parameters, arrow.wrap_in_whitespace(whitespace)));
        } else {
            opening_curly_brace = opening_curly_brace.wrap_in_whitespace(whitespace);
        }

        let (i, mut body) = body(i, indentation + 1);
        if !body.is_empty() {
            input = i;
        }

        let (i, whitespace) = whitespaces_and_newlines(i, indentation, true);
        if !body.is_empty() {
            let last = body.pop().unwrap();
            body.push(last.wrap_in_whitespace(whitespace));
        } else if let Some((parameters, arrow)) = parameters_and_arrow {
            parameters_and_arrow = Some((parameters, arrow.wrap_in_whitespace(whitespace)));
        } else {
            opening_curly_brace = opening_curly_brace.wrap_in_whitespace(whitespace);
        }

        let closing_curly_brace = match closing_curly_brace(i) {
            Some((i, closing_curly_brace)) => {
                input = i;
                closing_curly_brace
            }
            None => Rcst::Error {
                unparsable_input: "".to_string(),
                error: RcstError::CurlyBraceNotClosed,
            },
        };

        Some((
            input,
            Rcst::Lambda {
                opening_curly_brace: Box::new(opening_curly_brace),
                parameters_and_arrow: parameters_and_arrow
                    .map(|(parameters, arrow)| (parameters, Box::new(arrow))),
                body,
                closing_curly_brace: Box::new(closing_curly_brace),
            },
        ))
    }
    #[test]
    fn test_lambda() {
        assert_eq!(lambda("2", 0), None);
        assert_eq!(
            lambda("{ 2 }", 0),
            Some((
                "",
                Rcst::Lambda {
                    opening_curly_brace: Box::new(Rcst::OpeningCurlyBrace),
                    parameters_and_arrow: None,
                    body: vec![Rcst::Int(2)],
                    closing_curly_brace: Box::new(Rcst::ClosingCurlyBrace),
                }
            ))
        );
        // { a ->
        //   foo
        // }
        assert_eq!(
            lambda("{ a ->\n  foo\n}", 0),
            Some((
                "",
                Rcst::Lambda {
                    opening_curly_brace: Box::new(Rcst::OpeningCurlyBrace),
                    parameters_and_arrow: Some((
                        vec![Rcst::Identifier("a".to_string())],
                        Box::new(Rcst::Arrow)
                    )),
                    body: vec![Rcst::Identifier("foo".to_string())],
                    closing_curly_brace: Box::new(Rcst::ClosingCurlyBrace),
                }
            ))
        );
        // {
        // foo
        assert_eq!(
            lambda("{\nfoo", 0),
            Some((
                "\nfoo",
                Rcst::Lambda {
                    opening_curly_brace: Box::new(Rcst::OpeningCurlyBrace),
                    parameters_and_arrow: None,
                    body: vec![],
                    closing_curly_brace: Box::new(Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::CurlyBraceNotClosed
                    }),
                }
            ))
        );
        // {->
        // }
        assert_eq!(
            lambda("{->\n}", 1),
            Some((
                "\n}",
                Rcst::Lambda {
                    opening_curly_brace: Box::new(Rcst::OpeningCurlyBrace),
                    parameters_and_arrow: Some((vec![], Box::new(Rcst::Arrow))),
                    body: vec![],
                    closing_curly_brace: Box::new(Rcst::Error {
                        unparsable_input: "".to_string(),
                        error: RcstError::CurlyBraceNotClosed
                    }),
                }
            ))
        );
    }

    fn assignment(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
        log::trace!("assignment({:?}, {:?})", input, indentation);
        let (input, mut signature) = run_of_expressions(input, indentation)?;
        if signature.is_empty() {
            return None;
        }

        let (input, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
        let last = signature.pop().unwrap();
        signature.push(last.wrap_in_whitespace(whitespace.clone()));

        let parameters = signature.split_off(1);
        let name = signature.into_iter().next().unwrap();

        let (input, mut equals_sign) = equals_sign(input)?;
        let input_after_equals_sign = input;

        let (input, more_whitespace) = whitespaces_and_newlines(input, indentation, true);
        equals_sign = equals_sign.wrap_in_whitespace(more_whitespace.clone());

        let is_multiline = name.is_multiline()
            || parameters.is_multiline()
            || whitespace.is_multiline()
            || more_whitespace.is_multiline();
        let (input, body) = if is_multiline {
            let (input, whitespace) = leading_indentation(input, 1)?;
            equals_sign = equals_sign.wrap_in_whitespace(vec![whitespace]);

            let (input, body) = body(input, indentation + 1);
            if body.is_empty() {
                (input_after_equals_sign, body)
            } else {
                (input, body)
            }
        } else {
            match expression(input, indentation, true) {
                Some((input, expression)) => (input, vec![expression]),
                None => (input_after_equals_sign, vec![]),
            }
        };

        Some((
            input,
            Rcst::Assignment {
                name: Box::new(name),
                parameters,
                equals_sign: Box::new(equals_sign),
                body,
            },
        ))
    }
    #[test]
    fn test_assignment() {
        assert_eq!(
            assignment("foo = 42", 0),
            Some((
                "",
                Rcst::Assignment {
                    name: Box::new(Rcst::Identifier("foo".to_string())),
                    parameters: vec![],
                    equals_sign: Box::new(Rcst::EqualsSign),
                    body: vec![Rcst::Int(42)],
                }
            ))
        );
        assert_eq!(assignment("foo 42", 0), None);
        // foo bar =
        //   3
        // 2
        assert_eq!(
            assignment("foo bar =\n  3\n2", 0),
            Some((
                "\n2",
                Rcst::Assignment {
                    name: Box::new(Rcst::Identifier("foo".to_string())),
                    parameters: vec![Rcst::Identifier("bar".to_string())],
                    equals_sign: Box::new(Rcst::EqualsSign),
                    body: vec![Rcst::Int(3)],
                }
            ))
        );
        // foo
        //   bar
        //   = 3
        assert_eq!(
            assignment("foo bar\n  = 3", 0),
            Some((
                "",
                Rcst::Assignment {
                    name: Box::new(Rcst::Identifier("foo".to_string())),
                    parameters: vec![Rcst::Identifier("bar".to_string())],
                    equals_sign: Box::new(Rcst::EqualsSign),
                    body: vec![Rcst::Int(3)],
                }
            ))
        );
    }
}
