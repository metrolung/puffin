use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::iter::Peekable;
use std::ops::{Not, RangeTo, RangeToInclusive};
use std::str::{Chars, FromStr};
use std::sync::Mutex;
use anyhow::{anyhow, bail, Result};
use crate::compiler::error::CompilerError;
use crate::compiler::position::{PinPosition, Position, SpanPosition};
use crate::common::fsize::fsize;

pub enum Source {
    String(String),
}

impl Source {
    pub fn chars(&'_ self) -> Chars<'_> {
        match self {
            Source::String(str) => {
                str.chars()
            }
        }
    }

    pub fn get_row(&self, row: usize) -> String {
        let mut curr_row = 0;
        let mut line_str = String::new();

        for ch in self.chars() {
            if curr_row > row {
                break;
            }

            if ch == '\n' {
                curr_row += 1;
                continue;
            }

            if curr_row < row {
                continue;
            }

            line_str.push(ch);
        }

        line_str
    }

    pub fn error_message(&self, err: CompilerError) -> String {
        let line_text = self.get_row(err.span.first.row);
        let line_length = line_text.len();
        let pre_length = err.span.first.col;
        let max_arrow_length = line_length + 1 - pre_length;
        let arrow_length = err.span.len().min(max_arrow_length);

        format!(
            "{}\n{}\n{}{}",
            err.message,
            line_text,
            " ".repeat(pre_length),
            "^".repeat(arrow_length)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    EOF,

    Identifier(String),
    String(String),

    LParen,
    RParen,
    LCurly,
    RCurly,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,

    Plus,
    Minus,
    Multiply,
    Divide,
    Percent,
    Semicolon,
    Bang,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    If,
    Else,
    Let,
    Struct,
    Interface,
    Impl,
    Const,
    True,
    False,
    Return,
    Export,
    As,

    Integer(isize),
    UInteger(usize),
    Float(fsize),
    Char(char),
    Byte(u8),
}

#[derive(Debug, Clone)]
#[derive(PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SpanPosition,
}

impl Token {
    pub fn new(kind: TokenKind, span: SpanPosition) -> Self { Self { kind, span } }

    pub fn is_identifier(&self) -> bool {
        matches!(self.kind, TokenKind::Identifier(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self.kind,
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::UInteger(_)
        )
    }
}



struct LexContext<'ctx> {
    handled: bool,
    pin: PinPosition,
    chars: Peekable<Chars<'ctx>>,
    tokens: Vec<Token>,
}

#[derive(Debug, Copy, Clone)]
struct PinChar(PinPosition, char);
impl PinChar {
    fn char(&self) -> char { self.1 }
    fn pos(&self) -> PinPosition { self.0 }
}

impl Into<char> for PinChar {
    fn into(self) -> char {
        self.1
    }
}

impl<'ctx> LexContext<'ctx> {
    fn new(chars: Chars<'ctx>) -> Self {
        Self {
            handled: false,
            pin: PinPosition {
                row: 0,
                col: 0,
                idx: 0,
            },
            chars: chars.peekable(),
            tokens: vec![],
        }
    }

    fn next(&mut self) -> Option<PinChar> {
        let Some(next) = self.chars.next() else { return None };
        let next_pin = self.pin;

        self.pin.idx += 1;
        self.pin.col += 1;

        if next == '\n' {
            self.pin.row += 1;
            self.pin.col = 0;
        }

        Some(PinChar(next_pin, next))
    }

    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    fn add_token(&mut self, token: Token) {
        self.tokens.push(token);
        self.handled = true;
    }
}


fn read_identifier(ctx: &mut LexContext) -> Result<()> {
    let peeked = ctx.peek().unwrap();
    if !peeked.is_alphabetic() && peeked != &'_' {
        return Ok(());
    }

    let start = ctx.next().unwrap();

    let mut end_pos = start.pos();

    let mut identifier = start.char().to_string();

    while let Some(peeked) = ctx.peek() {
        if !peeked.is_alphanumeric() && peeked != &'_' { break };

        let next = ctx.next().unwrap();

        identifier.push(next.char());
        end_pos = next.pos();
    }

    let kind = match identifier.as_str() {
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "let" => TokenKind::Let,
        "impl" => TokenKind::Impl,
        "struct" => TokenKind::Struct,
        "const" => TokenKind::Const,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "return" => TokenKind::Return,
        "export" => TokenKind::Export,
        "as" => TokenKind::As,
        _ => TokenKind::Identifier(identifier),
    };

    let token = Token {
        span: start.pos().span(&end_pos),
        kind,
    };

    ctx.add_token(token);

    Ok(())
}

fn read_string(ctx: &mut LexContext) -> Result<()> {
    const CAP: char = '"';
    const ESCAPE: char = '\\';

    if *ctx.peek().unwrap() != CAP {
        return Ok(());
    }

    let start_cap = ctx.next().unwrap();
    let mut end_position = start_cap.pos();

    let mut string = String::new();

    loop {
        let Some(next) = ctx.next() else {
            bail!("unexpected EOF")
        };

        end_position = next.pos();

        if next.char() == CAP {
            break
        } else if next.char() == ESCAPE {
            let Some(next) = ctx.next() else {
                bail!("unexpected EOF")
            };

            match next.char() {
                '"' => string.push('"'),
                'n' => string.push('\n'),
                'r' => string.push('\r'),
                't' => string.push('\t'),
                '\'' => string.push('\''),
                '\\' => string.push('\\'),
                ch => bail!("unexpected escape character: {}", ch)
            };

        } else {
            string.push(next.char());
        }
    }

    let token = Token {
        span: start_cap.pos().span(&end_position),
        kind: TokenKind::String(string),
    };

    ctx.add_token(token);

    Ok(())
}

fn read_number(ctx: &mut LexContext) -> Result<()> {
    if !ctx.peek().unwrap().is_digit(10) {
        return Ok(());
    }

    let start = ctx.next().unwrap();
    let mut end_pos = start.pos();
    let mut decimal = false;

    let mut number = start.char().to_string();

    while let Some(peeked) = ctx.peek() {
        if !peeked.is_digit(10) {
            break;
        }

        let next = ctx.next().unwrap();
        end_pos = next.pos();
        number.push(next.char());
    }

    if ctx.peek() == Some(&'.') {
        end_pos = ctx.next().unwrap().pos();
        number.push('.');
        decimal = true;

        while let Some(peeked) = ctx.peek() {
            if !peeked.is_digit(10) {
                break;
            }

            let next = ctx.next().unwrap();
            end_pos = next.pos();
            number.push(next.char());
        }
    }

    let kind;

    if let Some(peeked) = ctx.peek() {
        match peeked {
            'u' => {
                end_pos = ctx.next().unwrap().pos();
                kind = TokenKind::UInteger(usize::from_str(&number)?);
            }
            'i' => {
                end_pos = ctx.next().unwrap().pos();
                kind = TokenKind::Integer(isize::from_str(&number)?);
            }
            'f' => {
                end_pos = ctx.next().unwrap().pos();
                kind = TokenKind::Float(fsize::from_str(&number)?);
            },
            _ => {
                if decimal {
                    kind = TokenKind::Float(fsize::from_str(&number)?);
                } else {
                    kind = TokenKind::Integer(isize::from_str(&number)?);
                }
            }
        }
    } else if decimal {
        kind = TokenKind::Float(fsize::from_str(&number)?);
    } else {
        kind = TokenKind::Integer(isize::from_str(&number)?);
    }

    let token = Token {
        span: start.pos().span(&end_pos),
        kind,
    };

    ctx.add_token(token);

    Ok(())
}

fn read_symbol_or_comment(ctx: &mut LexContext) -> Result<()> {
    let Some(peeked) = ctx.peek() else {
        return Ok(())
    };

    let kind = match peeked {
        '+' => TokenKind::Plus,
        '-' => TokenKind::Minus,
        '*' => TokenKind::Multiply,
        '/' => {
            let pos = ctx.next().unwrap().pos();

            if ctx.peek() == Some(&'/') {
                ctx.next();

                while ctx.peek() != Some(&'\n') {
                    ctx.next();
                }

                ctx.handled = true;

                return Ok(());
            }

            ctx.add_token(Token {
                span: pos.to_span(),
                kind: TokenKind::Divide,
            });

            return Ok(())
        },
        '%' => TokenKind::Percent,
        '!' => TokenKind::Bang,
        ':' => TokenKind::Colon,

        // i know this code is so redundant but im lazy
        '=' => {
            let pos = ctx.next().unwrap().pos();

            if ctx.peek() == Some(&'=') {
                let end_pos = ctx.next().unwrap().pos();

                ctx.add_token(Token { span: pos.span(&end_pos), kind: TokenKind::EqEq })
            } else {
                ctx.add_token(Token { span: pos.to_span(), kind: TokenKind::Eq })
            }
            return Ok(())
        },
        '<' => {
            let pos = ctx.next().unwrap().pos();

            if ctx.peek() == Some(&'=') {
                let end_pos = ctx.next().unwrap().pos();

                ctx.add_token(Token { span: pos.span(&end_pos), kind: TokenKind::Le })
            } else {
                ctx.add_token(Token { span: pos.to_span(), kind: TokenKind::Lt })
            }
            return Ok(())
        },
        '>' => {
            let pos = ctx.next().unwrap().pos();

            if ctx.peek() == Some(&'=') {
                let end_pos = ctx.next().unwrap().pos();

                ctx.add_token(Token { span: pos.span(&end_pos), kind: TokenKind::Ge })
            } else {
                ctx.add_token(Token { span: pos.to_span(), kind: TokenKind::Gt })
            }
            return Ok(())
        },

        '(' => TokenKind::LParen,
        ')' => TokenKind::RParen,
        '{' => TokenKind::LCurly,
        '}' => TokenKind::RCurly,
        '[' => TokenKind::LBracket,
        ']' => TokenKind::RBracket,
        ',' => TokenKind::Comma,
        ';' => TokenKind::Semicolon,
        '.' => TokenKind::Dot,
        _ => return Ok(()),
    };

    let pos = ctx.next().unwrap().pos();

    ctx.add_token(Token {
        span: pos.to_span(),
        kind,
    });

    Ok(())
}

fn skip_whitespace(ctx: &mut LexContext) {
    while let Some(ch) = ctx.peek() {
        if ch.is_whitespace() {
            ctx.next();
        } else {
            return;
        }
    }
}

pub fn lex(source: &Source) -> Result<Vec<Token>> {
    let mut ctx = LexContext::new(source.chars());

    while ctx.peek().is_some() {
        ctx.handled = false;

        skip_whitespace(&mut ctx);
        if ctx.peek().is_none() { break };
        read_identifier(&mut ctx)?;
        if ctx.peek().is_none() { break };
        read_string(&mut ctx)?;
        if ctx.peek().is_none() { break };
        read_number(&mut ctx)?;
        if ctx.peek().is_none() { break };
        read_symbol_or_comment(&mut ctx)?;

        if !ctx.handled { break };
    }

    if ctx.peek().is_some() {
        bail!("EOF expected")
    }

    ctx.add_token(
        Token {
            kind: TokenKind::EOF,
            span: ctx.pin.to_span()
        }
    );

    Ok(ctx.tokens)
}