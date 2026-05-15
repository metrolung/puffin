use std::iter::Peekable;
use std::slice::Iter;
use std::str::Chars;
use std::thread::spawn;
use anyhow::{anyhow, bail, Result};
use crate::compiler::lexer::{Token, TokenKind};
use crate::compiler::position::{Position, SpanPosition};
use crate::common::fsize::fsize;

#[derive(Debug)]
pub enum BinopKind {
    Add, Sub, Div, Mod, Mul
}

#[derive(Debug)]
pub enum ExprKind {
    Variable(String),
    AssignVariable(Option<Box<Expr>>, String, Box<Expr>),
    ReassignVariable(Box<Expr>, Box<Expr>),
    IdentifierAccess(Box<Expr>, String),
    IntegerAccess(Box<Expr>, usize),
    Tuple(Vec<Expr>),
    LitStr(String),
    LitInt(isize),
    LitUInt(usize),
    LitFloat(fsize),
    LitByte(u8),
    LitChar(char),
    LitBool(bool),
    LitUnit,
    Return(Box<Expr>),
    BinOp(Box<Expr>, Box<Expr>, BinopKind),
    Block(Vec<Expr>, Option<Box<Expr>>),
}

impl ExprKind {
    fn expr(self, span: SpanPosition) -> Expr {
        Expr { span, kind: self }
    }
}

#[derive(Debug)]
pub struct Expr {
    pub span: SpanPosition,
    pub kind: ExprKind,
}


#[derive(Debug)]
pub enum DefinitionKind {
    FunctionStatement {
        binding: Binding,
        param_bindings: Vec<Binding>,
        statement: Option<Expr>
    },
    StructStatement {
        name: String,
        fields: Vec<Binding>,
        implementations: Vec<Implementation>
    },
    ConstStatement {
        binding: Binding,
        value: Expr,
    },
    InterfaceStatement {
        name: String,
        functions: Vec<Definition>
    }
}

#[derive(Debug)]
pub struct Definition {
    pub span: SpanPosition,
    pub kind: DefinitionKind,
}

impl DefinitionKind {
    fn span(self, span: SpanPosition) -> Definition {
        Definition { kind: self, span }
    }
}

#[derive(Debug)]
pub struct Implementation {
    pub span: SpanPosition,
    pub implements: Expr,
    pub definitions: Vec<Definition>
}

#[derive(Debug)]
pub struct Binding {
    pub type_: Option<Expr>,
    pub name: String,
    pub span: SpanPosition,
}


pub struct ParseContext<'ctx> {
    tokens: Peekable<Iter<'ctx, Token>>,
    definitions: Vec<Definition>,
}

impl<'ctx> ParseContext<'ctx> {
    fn next(&mut self) -> Option<&'ctx Token> {
        self.tokens.next()
    }

    fn peek(&mut self) -> Option<&&'ctx Token> {
        self.tokens.peek()
    }
}

#[derive(Debug)]
pub struct AST {
    pub definitions: Vec<Definition>,
}

fn has_next(ctx: &mut ParseContext) -> bool {
    ctx.peek().is_some()
}

fn peek<'ctx>(ctx: &'ctx mut ParseContext) -> Result<&'ctx &'ctx Token> {
    ctx.peek().ok_or(anyhow!("expected token"))
}

fn next<'ctx>(ctx: &'ctx mut ParseContext) -> Result<&'ctx Token> {
    ctx.next().ok_or(anyhow!("expected token"))
}

fn eat<'ctx>(ctx: &'ctx mut ParseContext, kind: TokenKind) -> Result<&'ctx Token> {
    let token = next(ctx)?;

    if token.kind == kind {
        Ok(token)
    } else {
        bail!("expected token {:?}", kind)
    }
}

fn eat_identifier<'ctx>(ctx: &'ctx mut ParseContext) -> Result<(&'ctx String, SpanPosition)> {
    let token = next(ctx)?;

    match &token.kind {
        TokenKind::Identifier(ident) => Ok((ident, token.span)),
        _ => bail!("expected identifier"),
    }
}


fn is_upcoming(ctx: &mut ParseContext, kind: TokenKind) -> Result<bool> {
    let token = peek(ctx)?;

    if token.kind == kind {
        Ok(true)
    } else {
        Ok(false)
    }
}

fn is_any_upcoming(ctx: &mut ParseContext, kind: &[TokenKind]) -> Result<bool> {
    let token = peek(ctx)?;

    if kind.contains(&token.kind) {
        Ok(true)
    } else {
        Ok(false)
    }
}

fn tuple(ctx: &mut ParseContext) -> Result<Expr> {
    let start = eat(ctx, TokenKind::LParen)?.span;

    if is_upcoming(ctx, TokenKind::RParen)? {
        let end = next(ctx)?.span;
        return Ok(ExprKind::LitUnit.expr(start.span(&end)));
    }

    let value0 = expr(ctx)?;

    let comma_or_paren = next(ctx)?;

    match &comma_or_paren.kind {
        TokenKind::RParen => { Ok(value0) }
        TokenKind::Comma => {
            let mut values = vec![value0];

            while !is_upcoming(ctx, TokenKind::RParen)? {
                values.push(value(ctx)?);

                if !is_upcoming(ctx, TokenKind::Comma)? {
                    break;
                }

                eat(ctx, TokenKind::Comma)?;
            }

            let end = eat(ctx, TokenKind::RParen)?.span;

            Ok(ExprKind::Tuple(values).expr(start.span(&end)))
        }
        _ => bail!("expected , or )")
    }
}

fn block(ctx: &mut ParseContext) -> Result<Expr> {
    let start = eat(ctx, TokenKind::LCurly)?.span;

    let mut statements = vec![];

    while !is_upcoming(ctx, TokenKind::RCurly)? {
        if is_upcoming(ctx, TokenKind::Semicolon)? {
            next(ctx)?;
        } else {
            let next_statement = expr(ctx)?;
            if is_upcoming(ctx, TokenKind::RCurly)? {
                let end = next(ctx)?.span;
                let span = start.span(&end);

                return Ok(ExprKind::Block(statements, Some(Box::new(next_statement))).expr(span))
            }
            statements.push(next_statement);
            eat(ctx, TokenKind::Semicolon)?;
        }
    }

    let end = eat(ctx, TokenKind::RCurly)?.span;
    let span = start.span(&end);

    Ok(ExprKind::Block(statements, None).expr(span))
}

fn value(ctx: &mut ParseContext) -> Result<Expr> {
    if is_upcoming(ctx, TokenKind::LParen)? {
        tuple(ctx)
    } else if is_upcoming(ctx, TokenKind::LCurly)? {
        block(ctx)
    } else if is_upcoming(ctx, TokenKind::Return)? {
        let start = next(ctx)?.span;
        let expr = expr(ctx)?;
        let span = start.span(&expr.span);
        return Ok(ExprKind::Return(Box::new(expr)).expr(span));
    } else {
        let token = next(ctx)?;

        match &token.kind {
            TokenKind::Identifier(name) =>
                Ok(ExprKind::Variable(name.clone()).expr(token.span)),
            TokenKind::String(s) =>
                Ok(ExprKind::LitStr(s.clone()).expr(token.span)),
            TokenKind::Integer(i) =>
                Ok(ExprKind::LitInt(*i).expr(token.span)),
            TokenKind::UInteger(i) =>
                Ok(ExprKind::LitUInt(*i).expr(token.span)),
            TokenKind::Float(f) =>
                Ok(ExprKind::LitFloat(*f).expr(token.span)),
            TokenKind::Char(c) =>
                Ok(ExprKind::LitChar(*c).expr(token.span)),
            TokenKind::Byte(b) =>
                Ok(ExprKind::LitByte(*b).expr(token.span)),
            TokenKind::True =>
                Ok(ExprKind::LitBool(true).expr(token.span)),
            TokenKind::False =>
                Ok(ExprKind::LitBool(false).expr(token.span)),
            _ => bail!("expected value")
        }
    }
}

fn access(ctx: &mut ParseContext) -> Result<Expr> {
    let mut left = value(ctx)?;

    while peek(ctx)?.kind == TokenKind::Dot {
        next(ctx)?;

        let index = next(ctx)?;

        match &index.kind {
            TokenKind::Identifier(s) => {
                left = ExprKind::IdentifierAccess(Box::new(left), s.clone()).expr(index.span)
            }
            TokenKind::Integer(i) => {
                left = ExprKind::IntegerAccess(Box::new(left), (*i) as usize).expr(index.span)
            }
            TokenKind::Byte(i) => {
                left = ExprKind::IntegerAccess(Box::new(left), (*i) as usize).expr(index.span)
            }
            TokenKind::UInteger(i) => {
                left = ExprKind::IntegerAccess(Box::new(left), *i).expr(index.span)
            }
            _ => bail!("expected an identifier or an integer")
        }
    }

    Ok(left)
}

fn assignment(ctx: &mut ParseContext) -> Result<Expr> {
    if is_upcoming(ctx, TokenKind::Auto)? {
        let start = next(ctx)?.span;

        if let TokenKind::Identifier(name) = &next(ctx)?.kind {
            let name = name.clone();

            eat(ctx, TokenKind::Equals)?;
            let right = expr(ctx)?;
            let span = start.span(&right.span);

            return Ok(ExprKind::AssignVariable(
                None,
                name,
                Box::new(right)
            ).expr(span))
        } else {
            bail!("expected identifier")
        };
    }

    let left = access(ctx)?;
    let peeked = peek(ctx)?;

    if let TokenKind::Identifier(name) = &peeked.kind {
        let name = name.clone();
        next(ctx)?;

        eat(ctx, TokenKind::Equals)?;
        let right = expr(ctx)?;
        let span = left.span.span(&right.span);

        Ok(ExprKind::AssignVariable(
            Some(Box::new(left)),
            name.clone(),
            Box::new(right)
        ).expr(span))
    } else if let TokenKind::Equals = &peeked.kind {
        next(ctx)?;
        let right = expr(ctx)?;

        let position = left.span.span(&right.span);

        Ok(ExprKind::ReassignVariable(
            Box::new(left),
            Box::new(right)
        ).expr(position))
    } else {
        Ok(left)
    }
}

fn factors(ctx: &mut ParseContext) -> Result<Expr> {
    let mut left = assignment(ctx)?;

    while is_any_upcoming(ctx, &[TokenKind::Multiply, TokenKind::Divide, TokenKind::Percent])? {
        let token = &next(ctx)?.kind;

        let binop_kind = match token {
            TokenKind::Multiply => BinopKind::Mul,
            TokenKind::Divide => BinopKind::Div,
            TokenKind::Percent => BinopKind::Mod,
            _ => unreachable!()
        };

        let right = assignment(ctx)?;

        let position = left.span.span(&right.span);
        left = ExprKind::BinOp(Box::new(left), Box::new(right), binop_kind).expr(position);
    }

    Ok(left)
}

fn terms(ctx: &mut ParseContext) -> Result<Expr> {
    let mut left = factors(ctx)?;

    while is_any_upcoming(ctx, &[TokenKind::Plus, TokenKind::Minus])? {
        let token = &next(ctx)?.kind;

        let binop_kind = match token {
            TokenKind::Plus => BinopKind::Add,
            TokenKind::Minus => BinopKind::Sub,
            _ => unreachable!()
        };

        let right = factors(ctx)?;

        let position = left.span.span(&right.span);
        left = ExprKind::BinOp(Box::new(left), Box::new(right), binop_kind).expr(position);
    }

    Ok(left)
}

fn expr(ctx: &mut ParseContext) -> Result<Expr> {
    terms(ctx)
}

fn binding(ctx: &mut ParseContext) -> Result<Binding> {
    let (val_type, start_position) = if is_upcoming(ctx, TokenKind::Auto)? {
        let span = next(ctx)?.span;
        (None, span)
    } else {
        let type_node = value(ctx)?;
        let span = type_node.span;
        (Some(type_node), span)
    };

    let (name, end_position) = eat_identifier(ctx)?;

    Ok(Binding {
        type_: val_type,
        name: name.clone(),
        span: start_position.span(&end_position),
    })
}

fn func_statement(ctx: &mut ParseContext) -> Result<Definition> {
    let func_binding = binding(ctx)?;

    eat(ctx, TokenKind::LParen)?;

    let mut param_bindings = vec![];

    while !is_upcoming(ctx, TokenKind::RParen)? {
        param_bindings.push(binding(ctx)?);

        if !is_upcoming(ctx, TokenKind::Comma)? {
            break;
        }

        eat(ctx, TokenKind::Comma)?;
    }

    let end_position = eat(ctx, TokenKind::RParen)?.span;

    if !is_upcoming(ctx, TokenKind::Semicolon)? {
        let block = block(ctx)?;
        let span = func_binding.span.span(&block.span);

        Ok(DefinitionKind::FunctionStatement {
            binding: func_binding,
            param_bindings,
            statement: Some(block),
        }.span(span))
    } else {
        let span = func_binding.span.span(&end_position);

        Ok(DefinitionKind::FunctionStatement {
            binding: func_binding,
            param_bindings,
            statement: None,
        }.span(span))
    }
}

fn interface_statement(ctx: &mut ParseContext) -> Result<Definition> {
    let start_position = eat(ctx, TokenKind::Interface)?.span;

    let name = eat_identifier(ctx)?.0.clone();

    eat(ctx, TokenKind::LCurly)?;

    let mut functions = vec![];

    while !is_upcoming(ctx, TokenKind::RParen)? {
        let func = func_statement(ctx)?;
        functions.push(func);
    }

    let end_position = eat(ctx, TokenKind::RParen)?.span;

    let span = start_position.span(&end_position);
    Ok(DefinitionKind::InterfaceStatement {
        name,
        functions,
    }.span(span))
}

fn struct_statement(ctx: &mut ParseContext) -> Result<Definition> {
    let start_position = eat(ctx, TokenKind::Struct)?.span;

    let name = eat_identifier(ctx)?.0.clone();

    eat(ctx, TokenKind::LCurly)?;

    let mut fields = vec![];
    let mut implementations = vec![];

    while !is_upcoming(ctx, TokenKind::RCurly)? {
        if is_upcoming(ctx, TokenKind::Impl)? {
            let start = eat(ctx, TokenKind::Impl)?.span;

            let implements = expr(ctx)?;
            let mut functions = vec![];

            let mut last = implements.span;

            while !is_upcoming(ctx, TokenKind::RCurly)? && !is_upcoming(ctx, TokenKind::Impl)? {
                let func = func_statement(ctx)?;
                last = func.span;
                functions.push(func);
            }

            implementations.push(Implementation {
                span: start.span(&last),
                implements,
                definitions: vec![],
            })
        } else {
            fields.push(binding(ctx)?);
            if is_upcoming(ctx, TokenKind::Comma)? {
                next(ctx)?;
            } else if !is_any_upcoming(ctx, &[TokenKind::RCurly, TokenKind::Impl])? {
                bail!("expected }} or impl")
            }
        }
    }

    let end_position = eat(ctx, TokenKind::RCurly)?.span;

    let span = start_position.span(&end_position);
    Ok(DefinitionKind::StructStatement {
        name,
        fields,
        implementations: vec![],
    }.span(span))
}

fn const_statement(ctx: &mut ParseContext) -> Result<Definition> {
    eat(ctx, TokenKind::Const)?;

    let binding = binding(ctx)?;

    eat(ctx, TokenKind::Equals)?;

    let expr = expr(ctx)?;

    let span = binding.span.span(&expr.span);

    eat(ctx, TokenKind::Semicolon)?;

    Ok(DefinitionKind::ConstStatement {
        binding,
        value: expr,
    }.span(span))
}

fn definition(ctx: &mut ParseContext) -> Result<Definition> {
    let Some(peeked) = ctx.peek() else {
        bail!("early eof")
    };

    if peeked.kind == TokenKind::Struct {
        struct_statement(ctx)
    } else if peeked.kind == TokenKind::Interface {
        interface_statement(ctx)
    } else if peeked.kind == TokenKind::Const {
        const_statement(ctx)
    } else if matches!(peeked.kind, TokenKind::LParen | TokenKind::Identifier(..)) {
        func_statement(ctx)
    } else {
        bail!("expected definition")
    }
}


pub fn parse(nodes: &Vec<Token>) -> Result<AST> {
    let mut ctx = ParseContext {
        tokens: nodes.iter().peekable(),
        definitions: vec![]
    };

    while !is_upcoming(&mut ctx, TokenKind::EOF)? {
        if is_upcoming(&mut ctx, TokenKind::Semicolon)? {
            next(&mut ctx)?;
        } else {
            let definition = definition(&mut ctx)?;
            ctx.definitions.push(definition);
        }
    }

    eat(&mut ctx, TokenKind::EOF)?;

    Ok(AST {
        definitions: ctx.definitions,
    })
}