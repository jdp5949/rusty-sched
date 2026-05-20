//! Hand-written recursive-descent parser for condition expressions.
//!
//! Grammar (precedence: not > and > or):
//! ```text
//! expr     = or_expr
//! or_expr  = and_expr ('or' and_expr)*
//! and_expr = not_expr ('and' not_expr)*
//! not_expr = 'not' atom | atom
//! atom     = '(' expr ')' | func_call
//! func_call = IDENT '(' job_name ')' [cmp_op NUMBER]
//!           | 'value' '(' name ')'
//! ```

use crate::expr::{CmpOp, Expr};
use thiserror::Error;

/// Error returned by [`parse`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Unexpected token or end of input.
    #[error("unexpected token at position {pos}: {msg}")]
    Unexpected {
        /// Byte position in the input.
        pos: usize,
        /// Description of what went wrong.
        msg: String,
    },
    /// Unrecognized function name.
    #[error("unknown function '{name}'")]
    UnknownFunction {
        /// The bad function name.
        name: String,
    },
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input.get(self.pos).map(|&b| b as char)
    }

    fn consume_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += 1;
        Some(c)
    }

    fn expect_char(&mut self, expected: char) -> Result<(), ParseError> {
        self.skip_ws();
        match self.consume_char() {
            Some(c) if c == expected => Ok(()),
            _ => Err(ParseError::Unexpected {
                pos: self.pos,
                msg: format!("expected '{expected}'"),
            }),
        }
    }

    /// Read an identifier (letters, digits, underscores, hyphens, dots).
    fn read_ident(&mut self) -> String {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.input[start..self.pos]).into_owned()
    }

    /// Parse a job name inside parens: `(job_name)`. Returns the name.
    fn parse_paren_name(&mut self) -> Result<String, ParseError> {
        self.expect_char('(')?;
        let name = self.read_ident();
        if name.is_empty() {
            return Err(ParseError::Unexpected {
                pos: self.pos,
                msg: "expected job name".into(),
            });
        }
        self.skip_ws();
        self.expect_char(')')?;
        Ok(name)
    }

    /// Try to read a comparison op: `=`, `!=`, `<`, `<=`, `>`, `>=`.
    fn try_read_cmp_op(&mut self) -> Option<CmpOp> {
        self.skip_ws();
        let remaining = &self.input[self.pos..];
        if remaining.starts_with(b"!=") {
            self.pos += 2;
            Some(CmpOp::Ne)
        } else if remaining.starts_with(b"<=") {
            self.pos += 2;
            Some(CmpOp::Le)
        } else if remaining.starts_with(b">=") {
            self.pos += 2;
            Some(CmpOp::Ge)
        } else if remaining.starts_with(b"=") {
            self.pos += 1;
            Some(CmpOp::Eq)
        } else if remaining.starts_with(b"<") {
            self.pos += 1;
            Some(CmpOp::Lt)
        } else if remaining.starts_with(b">") {
            self.pos += 1;
            Some(CmpOp::Gt)
        } else {
            None
        }
    }

    /// Read a (possibly negative) integer.
    fn read_int(&mut self) -> Result<i32, ParseError> {
        self.skip_ws();
        let negative = if self.input.get(self.pos) == Some(&b'-') {
            self.pos += 1;
            true
        } else {
            false
        };
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.input[start..self.pos]).unwrap();
        if s.is_empty() {
            return Err(ParseError::Unexpected {
                pos: self.pos,
                msg: "expected integer".into(),
            });
        }
        let n: i32 = s.parse().map_err(|_| ParseError::Unexpected {
            pos: self.pos,
            msg: "integer overflow".into(),
        })?;
        Ok(if negative { -n } else { n })
    }

    fn parse_func_call(&mut self, func: &str) -> Result<Expr, ParseError> {
        let func_lower = func.to_lowercase();
        match func_lower.as_str() {
            "success" | "s" => Ok(Expr::Success(self.parse_paren_name()?)),
            "failure" | "f" => Ok(Expr::Failure(self.parse_paren_name()?)),
            "done" | "d" => Ok(Expr::Done(self.parse_paren_name()?)),
            "running" | "r" => Ok(Expr::Running(self.parse_paren_name()?)),
            "notrunning" | "n" => Ok(Expr::NotRunning(self.parse_paren_name()?)),
            "exitcode" => {
                let job = self.parse_paren_name()?;
                let op = self
                    .try_read_cmp_op()
                    .ok_or_else(|| ParseError::Unexpected {
                        pos: self.pos,
                        msg: "expected comparison operator after exitcode(...)".into(),
                    })?;
                let n = self.read_int()?;
                Ok(Expr::ExitCode(job, op, n))
            }
            "value" => Ok(Expr::Value(self.parse_paren_name()?)),
            _ => Err(ParseError::UnknownFunction {
                name: func.to_string(),
            }),
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        if self.peek_char() == Some('(') {
            self.pos += 1;
            let inner = self.parse_or()?;
            self.expect_char(')')?;
            return Ok(inner);
        }
        let ident = self.read_ident();
        if ident.is_empty() {
            return Err(ParseError::Unexpected {
                pos: self.pos,
                msg: "expected expression".into(),
            });
        }
        // Peek: if next non-ws char is '(', it's a function call.
        self.skip_ws();
        if self.peek_char() == Some('(') {
            self.parse_func_call(&ident)
        } else {
            Err(ParseError::Unexpected {
                pos: self.pos,
                msg: format!("unexpected identifier '{ident}' without call syntax"),
            })
        }
    }

    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        // Peek ahead: is the next ident "not"?
        let saved = self.pos;
        let ident = self.read_ident();
        if ident.eq_ignore_ascii_case("not") {
            // It must be followed by a non-'(' token to be the 'not' keyword
            // (so it doesn't collide with a function named "not").
            self.skip_ws();
            if self.peek_char() != Some('(') {
                // It's the 'not' keyword; parse atom.
                return Ok(Expr::Not(Box::new(self.parse_not()?)));
            }
        }
        // Restore and fall through.
        self.pos = saved;
        self.parse_atom()
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_not()?;
        loop {
            let saved = self.pos;
            self.skip_ws();
            let tok = self.read_ident();
            if tok.eq_ignore_ascii_case("and") {
                let right = self.parse_not()?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else {
                self.pos = saved;
                break;
            }
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        loop {
            let saved = self.pos;
            self.skip_ws();
            let tok = self.read_ident();
            if tok.eq_ignore_ascii_case("or") {
                let right = self.parse_and()?;
                left = Expr::Or(Box::new(left), Box::new(right));
            } else {
                self.pos = saved;
                break;
            }
        }
        Ok(left)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_or()?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err(ParseError::Unexpected {
                pos: self.pos,
                msg: format!(
                    "trailing input: '{}'",
                    String::from_utf8_lossy(&self.input[self.pos..])
                ),
            });
        }
        Ok(expr)
    }
}

/// Parse a condition expression string into an [`Expr`] AST.
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    Parser::new(input).parse_expr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{CmpOp, Expr};

    #[test]
    fn simple_success() {
        let e = parse("success(jobA)").unwrap();
        assert_eq!(e, Expr::Success("jobA".into()));
    }

    #[test]
    fn short_alias_s() {
        let e = parse("s(jobA)").unwrap();
        assert_eq!(e, Expr::Success("jobA".into()));
    }

    #[test]
    fn failure_alias() {
        assert_eq!(parse("f(x)").unwrap(), Expr::Failure("x".into()));
        assert_eq!(parse("failure(x)").unwrap(), Expr::Failure("x".into()));
    }

    #[test]
    fn done_alias() {
        assert_eq!(parse("d(x)").unwrap(), Expr::Done("x".into()));
        assert_eq!(parse("done(x)").unwrap(), Expr::Done("x".into()));
    }

    #[test]
    fn running_alias() {
        assert_eq!(parse("r(x)").unwrap(), Expr::Running("x".into()));
        assert_eq!(parse("running(x)").unwrap(), Expr::Running("x".into()));
    }

    #[test]
    fn notrunning_alias() {
        assert_eq!(parse("n(x)").unwrap(), Expr::NotRunning("x".into()));
        assert_eq!(
            parse("notrunning(x)").unwrap(),
            Expr::NotRunning("x".into())
        );
    }

    #[test]
    fn exitcode_ops() {
        assert_eq!(
            parse("exitcode(j) = 0").unwrap(),
            Expr::ExitCode("j".into(), CmpOp::Eq, 0)
        );
        assert_eq!(
            parse("exitcode(j) != 1").unwrap(),
            Expr::ExitCode("j".into(), CmpOp::Ne, 1)
        );
        assert_eq!(
            parse("exitcode(j) >= 2").unwrap(),
            Expr::ExitCode("j".into(), CmpOp::Ge, 2)
        );
    }

    #[test]
    fn and_expr() {
        let e = parse("success(a) and failure(b)").unwrap();
        assert_eq!(
            e,
            Expr::And(
                Box::new(Expr::Success("a".into())),
                Box::new(Expr::Failure("b".into()))
            )
        );
    }

    #[test]
    fn or_expr() {
        let e = parse("done(a) or running(b)").unwrap();
        assert_eq!(
            e,
            Expr::Or(
                Box::new(Expr::Done("a".into())),
                Box::new(Expr::Running("b".into()))
            )
        );
    }

    #[test]
    fn not_expr() {
        let e = parse("not success(a)").unwrap();
        assert_eq!(e, Expr::Not(Box::new(Expr::Success("a".into()))));
    }

    #[test]
    fn nested_parens() {
        let e = parse("success(a) and (failure(b) or done(c))").unwrap();
        assert_eq!(
            e,
            Expr::And(
                Box::new(Expr::Success("a".into())),
                Box::new(Expr::Or(
                    Box::new(Expr::Failure("b".into())),
                    Box::new(Expr::Done("c".into()))
                ))
            )
        );
    }

    #[test]
    fn complex_autosys_style() {
        let e =
            parse("success(jobA) and (failure(jobB) or done(jobC)) and notrunning(jobD)").unwrap();
        // Just ensure it parses without error.
        assert!(matches!(e, Expr::And(_, _)));
    }

    #[test]
    fn case_insensitive_keywords() {
        let e = parse("success(a) AND success(b)").unwrap();
        assert!(matches!(e, Expr::And(_, _)));

        let e = parse("success(a) OR success(b)").unwrap();
        assert!(matches!(e, Expr::Or(_, _)));
    }

    #[test]
    fn value_expr() {
        let e = parse("value(global.x)").unwrap();
        assert_eq!(e, Expr::Value("global.x".into()));
    }

    #[test]
    fn unknown_func_error() {
        let err = parse("foo(x)").unwrap_err();
        assert!(matches!(err, ParseError::UnknownFunction { .. }));
    }

    #[test]
    fn display_roundtrip() {
        let inputs = [
            "success(jobA)",
            "failure(jobB)",
            "exitcode(j) = 0",
            "notrunning(x)",
        ];
        for s in inputs {
            let e = parse(s).unwrap();
            let displayed = e.to_string();
            let reparsed = parse(&displayed).unwrap();
            assert_eq!(e, reparsed, "roundtrip failed for: {s}");
        }
    }
}
