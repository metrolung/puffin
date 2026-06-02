use std::iter::Peekable;
use std::slice::Iter;
use std::str::Chars;
use std::thread::spawn;
use anyhow::{anyhow, bail, Result};
use crate::compiler::lexer::{Token, TokenKind};
use crate::compiler::position::{Position, SpanPosition};

#[derive(Debug, Clone)]
pub enum BinOpKind {
    Add, Sub, Mul, Div,
    Eq, Ne,
    Lt,
    Le, Gt,
    Ge
}

#[derive(Debug, Clone)]
pub enum UnOpKind {
    Pos, Neg,
    Not
}

#[derive(Debug)]
pub enum ValueExprKind {
    Variable(String),
    AssignVariable(String, Option<Box<TypeExpr>>, Box<ValueExpr>),
    ReassignVariable(Box<ValueExpr>, Box<ValueExpr>),
    IdentifierAccess(Box<ValueExpr>, String),
    IntegerAccess(Box<ValueExpr>, usize),
    Tuple(Vec<ValueExpr>),
    Call(Box<ValueExpr>, Vec<ValueExpr>),
    LitStr(String),
    LitInt(i64),
    LitUInt(u64),
    LitFloat(f64),
    LitChar(char),
    LitBool(bool),
    Return(Option<Box<ValueExpr>>),
    Cast(Box<ValueExpr>, Box<TypeExpr>),
    BinOp(BinOpKind, Box<ValueExpr>, Box<ValueExpr>),
    UnOp(UnOpKind, Box<ValueExpr>),
    Block(Vec<ValueExpr>, Option<Box<ValueExpr>>),
    If(Box<ValueExpr>, Box<ValueExpr>, Option<Box<ValueExpr>>),
}

impl ValueExprKind {
    fn expr(self, span: SpanPosition) -> ValueExpr {
        ValueExpr { span, kind: self }
    }
}

#[derive(Debug)]
pub struct ValueExpr {
    pub span: SpanPosition,
    pub kind: ValueExprKind,
}

#[derive(Debug)]
pub enum TypeExprKind {
    Variable(String),
    Tuple(Vec<TypeExpr>),
}

impl TypeExprKind {
    fn expr(self, span: SpanPosition) -> TypeExpr {
        TypeExpr { span, kind: self }
    }
}

#[derive(Debug)]
pub struct TypeExpr {
    pub span: SpanPosition,
    pub kind: TypeExprKind,
}


#[derive(Debug)]
pub enum StatementKind {
    FunctionStatement {
        name: String,
        return_type: Option<TypeExpr>,
        param_bindings: Vec<(String, TypeExpr)>,
        statement: Option<ValueExpr>,
        export: bool
    },
    StructStatement {
        name: String,
        fields: Vec<(String, TypeExpr)>,
        implementations: Vec<Implementation>
    },
    ConstStatement {
        name: String,
        const_type: TypeExpr,
        value: ValueExpr,
    },
    InterfaceStatement {
        name: String,
        functions: Vec<Statement>
    }
}

#[derive(Debug)]
pub struct Statement {
    pub span: SpanPosition,
    pub kind: StatementKind,
}

impl StatementKind {
    fn statement(self, span: SpanPosition) -> Statement {
        Statement { kind: self, span }
    }
}

#[derive(Debug)]
pub struct Implementation {
    pub span: SpanPosition,
    pub implements: ValueExpr,
    pub definitions: Vec<Statement>
}

pub struct ParseContext<'ctx> {
    tokens: Peekable<Iter<'ctx, Token>>,
    definitions: Vec<Statement>,
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
    pub definitions: Vec<Statement>,
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

fn type_tuple(ctx: &mut ParseContext) -> Result<TypeExpr> {
    let start = eat(ctx, TokenKind::LBracket)?.span;

    if is_upcoming(ctx, TokenKind::RBracket)? {
        let end = next(ctx)?.span;
        return Ok(TypeExprKind::Tuple(vec![]).expr(start.span(&end)));
    }

    let value0 = type_expr(ctx)?;

    let comma_or_paren = next(ctx)?;

    match &comma_or_paren.kind {
        TokenKind::RParen => { Ok(value0) }
        TokenKind::Comma => {
            let mut values = vec![value0];

            while !is_upcoming(ctx, TokenKind::RBracket)? {
                values.push(type_expr(ctx)?);

                if !is_upcoming(ctx, TokenKind::Comma)? {
                    break;
                }

                eat(ctx, TokenKind::Comma)?;
            }

            let end = eat(ctx, TokenKind::RBracket)?.span;

            Ok(TypeExprKind::Tuple(values).expr(start.span(&end)))
        }
        _ => bail!("expected , or )")
    }
}

fn value_tuple(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let start = eat(ctx, TokenKind::LParen)?.span;

    if is_upcoming(ctx, TokenKind::RParen)? {
        let end = next(ctx)?.span;
        return Ok(ValueExprKind::Tuple(vec![]).expr(start.span(&end)));
    }

    let value0 = value_expr(ctx)?;

    let comma_or_paren = next(ctx)?;

    match &comma_or_paren.kind {
        TokenKind::RParen => { Ok(value0) }
        TokenKind::Comma => {
            let mut values = vec![value0];

            while !is_upcoming(ctx, TokenKind::RParen)? {
                values.push(value_expr(ctx)?);

                if !is_upcoming(ctx, TokenKind::Comma)? {
                    break;
                }

                eat(ctx, TokenKind::Comma)?;
            }

            let end = eat(ctx, TokenKind::RParen)?.span;

            Ok(ValueExprKind::Tuple(values).expr(start.span(&end)))
        }
        _ => bail!("expected , or ]")
    }
}

fn block(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let start = eat(ctx, TokenKind::LCurly)?.span;

    let mut statements = vec![];

    while !is_upcoming(ctx, TokenKind::RCurly)? {
        if is_upcoming(ctx, TokenKind::Semicolon)? {
            next(ctx)?;
        } else {
            let next_statement = value_expr(ctx)?;
            if is_upcoming(ctx, TokenKind::RCurly)? {
                let end = next(ctx)?.span;
                let span = start.span(&end);

                return Ok(ValueExprKind::Block(statements, Some(Box::new(next_statement))).expr(span))
            }
            statements.push(next_statement);
            eat(ctx, TokenKind::Semicolon)?;
        }
    }

    let end = eat(ctx, TokenKind::RCurly)?.span;
    let span = start.span(&end);

    Ok(ValueExprKind::Block(statements, None).expr(span))
}

fn type_expr(ctx: &mut ParseContext) -> Result<TypeExpr> {
    if is_upcoming(ctx, TokenKind::LBracket)? {
        type_tuple(ctx)
    } else {
        let token = next(ctx)?;

        match &token.kind {
            TokenKind::Identifier(name) =>
                Ok(TypeExprKind::Variable(name.clone()).expr(token.span)),
            _ => bail!("expected type")
        }
    }
}

fn value(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let peeked = &peek(ctx)?.kind;

    match peeked {
        TokenKind::LParen => value_tuple(ctx),
        TokenKind::LCurly => block(ctx),
        TokenKind::Plus | TokenKind::Minus | TokenKind::Bang => {
            let op = next(ctx)?;
            let unop_kind = match op.kind {
                TokenKind::Plus => UnOpKind::Pos,
                TokenKind::Minus => UnOpKind::Neg,
                TokenKind::Bang => UnOpKind::Not,
                _ => unreachable!()
            };

            let start = op.span;
            let value = access(ctx)?;

            let span = start.span(&value.span);

            Ok(ValueExprKind::UnOp(unop_kind, Box::new(value)).expr(span))
        }
        TokenKind::Return => {
            let start = next(ctx)?.span;

            if is_any_upcoming(ctx, &[TokenKind::Semicolon, TokenKind::RCurly])? {
                let end = next(ctx)?.span;
                let span = start.span(&end);

                return Ok(ValueExprKind::Return(None).expr(span))
            }

            let expr = value_expr(ctx)?;
            let span = start.span(&expr.span);
            Ok(ValueExprKind::Return(Some(Box::new(expr))).expr(span))
        }
        TokenKind::If => {
            let start = next(ctx)?.span;

            let cond = if is_upcoming(ctx, TokenKind::Bang)? {
                let start = next(ctx)?.span;
                eat(ctx, TokenKind::LParen)?;
                let cond = value_expr(ctx)?;
                let end = eat(ctx, TokenKind::RParen)?.span;
                ValueExprKind::UnOp(UnOpKind::Neg, Box::new(cond)).expr(start.span(&end))
            } else {
                eat(ctx, TokenKind::LParen)?;
                let cond = value_expr(ctx)?;
                eat(ctx, TokenKind::RParen)?;
                cond
            };

            let block = value_expr(ctx)?;

            if is_upcoming(ctx, TokenKind::Else)? {
                next(ctx)?;
                let else_block = value_expr(ctx)?;
                let span = start.span(&else_block.span);
                Ok(ValueExprKind::If(Box::new(cond), Box::new(block), Some(Box::new(else_block))).expr(span))
            } else {
                let span = start.span(&block.span);
                Ok(ValueExprKind::If(Box::new(cond), Box::new(block), None).expr(span))
            }
        }
        _ => {
            let token = next(ctx)?;

            match &token.kind {
                TokenKind::Identifier(name) =>
                    Ok(ValueExprKind::Variable(name.clone()).expr(token.span)),
                TokenKind::String(s) =>
                    Ok(ValueExprKind::LitStr(s.clone()).expr(token.span)),
                TokenKind::Integer(i) =>
                    Ok(ValueExprKind::LitInt(*i).expr(token.span)),
                TokenKind::UInteger(i) =>
                    Ok(ValueExprKind::LitUInt(*i).expr(token.span)),
                TokenKind::Float(f) =>
                    Ok(ValueExprKind::LitFloat(*f).expr(token.span)),
                TokenKind::Char(c) =>
                    Ok(ValueExprKind::LitChar(*c).expr(token.span)),
                TokenKind::True =>
                    Ok(ValueExprKind::LitBool(true).expr(token.span)),
                TokenKind::False =>
                    Ok(ValueExprKind::LitBool(false).expr(token.span)),
                _ => bail!("expected value")
            }
        }
    }
}

fn access(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let mut left = value(ctx)?;

    while is_upcoming(ctx, TokenKind::Dot)? || is_upcoming(ctx, TokenKind::LParen)? {
        match next(ctx)?.kind {
            TokenKind::Dot => {
                let index = next(ctx)?;

                match &index.kind {
                    TokenKind::Identifier(s) => {
                        left = ValueExprKind::IdentifierAccess(Box::new(left), s.clone()).expr(index.span)
                    }
                    TokenKind::Integer(i) => {
                        left = ValueExprKind::IntegerAccess(Box::new(left), *i as usize).expr(index.span)
                    }
                    TokenKind::UInteger(i) => {
                        left = ValueExprKind::IntegerAccess(Box::new(left), *i as usize).expr(index.span)
                    }
                    _ => bail!("expected an identifier or an integer")
                }
            }
            TokenKind::LParen => {
                let mut params = vec![];
                let mut end = left.span;

                while !is_upcoming(ctx, TokenKind::RParen)? {
                    let expr = value_expr(ctx)?;
                    end = expr.span;
                    params.push(expr);

                    if is_upcoming(ctx, TokenKind::Comma)? {
                        next(ctx)?;
                    } else {
                        break
                    }
                }

                eat(ctx, TokenKind::RParen)?;

                let span = left.span.span(&end);
                left = ValueExprKind::Call(Box::new(left), params).expr(span);
            }
            _ => unreachable!()
        };
    }

    Ok(left)
}

fn casts(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let mut left = access(ctx)?;

    while is_upcoming(ctx, TokenKind::As)? {
        next(ctx)?;

        let right = type_expr(ctx)?;

        let position = left.span.span(&right.span);
        left = ValueExprKind::Cast(Box::new(left), Box::new(right)).expr(position);
    }

    Ok(left)
}

fn factors(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let mut left = casts(ctx)?;

    while is_any_upcoming(ctx, &[TokenKind::Multiply, TokenKind::Divide])? {
        let token = &next(ctx)?.kind;

        let binop_kind = match token {
            TokenKind::Multiply => BinOpKind::Mul,
            TokenKind::Divide => BinOpKind::Div,
            _ => unreachable!()
        };

        let right = casts(ctx)?;

        let position = left.span.span(&right.span);
        left = ValueExprKind::BinOp(binop_kind, Box::new(left), Box::new(right)).expr(position);
    }

    Ok(left)
}

fn terms(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let mut left = factors(ctx)?;

    while is_any_upcoming(ctx, &[TokenKind::Plus, TokenKind::Minus])? {
        let token = &next(ctx)?.kind;

        let binop_kind = match token {
            TokenKind::Plus => BinOpKind::Add,
            TokenKind::Minus => BinOpKind::Sub,
            _ => unreachable!()
        };

        let right = factors(ctx)?;

        let position = left.span.span(&right.span);
        left = ValueExprKind::BinOp(binop_kind, Box::new(left), Box::new(right)).expr(position);
    }

    Ok(left)
}

fn comparison(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let mut left = terms(ctx)?;

    while is_any_upcoming(ctx, &[TokenKind::Lt, TokenKind::Le, TokenKind::Gt, TokenKind::Ge])? {
        let token = &next(ctx)?.kind;

        let binop_kind = match token {
            TokenKind::Lt => BinOpKind::Lt,
            TokenKind::Le => BinOpKind::Le,
            TokenKind::Gt => BinOpKind::Gt,
            TokenKind::Ge => BinOpKind::Ge,
            _ => unreachable!()
        };

        let right = terms(ctx)?;

        let position = left.span.span(&right.span);
        left = ValueExprKind::BinOp(binop_kind, Box::new(left), Box::new(right)).expr(position);
    }

    Ok(left)
}

fn equality(ctx: &mut ParseContext) -> Result<ValueExpr> {
    let mut left = comparison(ctx)?;

    while is_any_upcoming(ctx, &[TokenKind::EqEq, TokenKind::Ne])? {
        let token = &next(ctx)?.kind;

        let binop_kind = match token {
            TokenKind::EqEq => BinOpKind::Eq,
            TokenKind::Ne => BinOpKind::Ne,
            _ => unreachable!()
        };

        let right = comparison(ctx)?;

        let position = left.span.span(&right.span);
        left = ValueExprKind::BinOp(binop_kind, Box::new(left), Box::new(right)).expr(position);
    }

    Ok(left)
}

fn assignment(ctx: &mut ParseContext) -> Result<ValueExpr> {
    if is_upcoming(ctx, TokenKind::Let)? {
        next(ctx)?;

        let (name, start_position) = eat_identifier(ctx)?;
        let name = name.clone();

        if is_upcoming(ctx, TokenKind::Eq)? {
            next(ctx)?;

            let value = value_expr(ctx)?;
            let position = start_position.span(&value.span);

            Ok(ValueExprKind::AssignVariable(
                name.clone(),
                None,
                Box::new(value)
            ).expr(position))
        } else {
            let ty = type_expr(ctx)?;

            eat(ctx, TokenKind::Eq)?;

            let value = value_expr(ctx)?;
            let position = start_position.span(&value.span);

            Ok(ValueExprKind::AssignVariable(
                name.clone(),
                Some(Box::new(ty)),
                Box::new(value)
            ).expr(position))
        }
    } else {
        let place = equality(ctx)?;

        if is_upcoming(ctx, TokenKind::Eq)? {
            next(ctx)?;

            let value = value_expr(ctx)?;
            let position = place.span.span(&value.span);

            Ok(ValueExprKind::ReassignVariable(
                Box::new(place),
                Box::new(value)
            ).expr(position))
        } else {
            Ok(place)
        }
    }
}

fn value_expr(ctx: &mut ParseContext) -> Result<ValueExpr> {
    assignment(ctx)
}

fn func_statement(ctx: &mut ParseContext) -> Result<Statement> {
    let exposed = if is_upcoming(ctx, TokenKind::Export)? {
        next(ctx)?;
        true
    } else {
        false
    };

    let (func_name, start_position) = eat_identifier(ctx)?;
    let func_name = func_name.clone();

    eat(ctx, TokenKind::LParen)?;

    let mut param_bindings = vec![];

    while !is_upcoming(ctx, TokenKind::RParen)? {
        let (name, ..) = eat_identifier(ctx)?;
        let name = name.clone();

        let ty = type_expr(ctx)?;

        param_bindings.push((name, ty));

        if !is_upcoming(ctx, TokenKind::Comma)? {
            break;
        }

        eat(ctx, TokenKind::Comma)?;
    }

    let end_position = eat(ctx, TokenKind::RParen)?.span;

    if is_upcoming(ctx, TokenKind::Semicolon)? {
        let position = start_position.span(&end_position);

        return Ok(StatementKind::FunctionStatement {
            name: func_name,
            return_type: None,
            param_bindings,
            statement: None,
            export: exposed,
        }.statement(position))
    }

    if is_upcoming(ctx, TokenKind::LCurly)? {
        let block = block(ctx)?;
        let position = start_position.span(&block.span);

        return Ok(StatementKind::FunctionStatement {
            name: func_name,
            return_type: None,
            param_bindings,
            statement: Some(block),
            export: exposed,
        }.statement(position));
    }

    let return_type = type_expr(ctx)?;

    if is_upcoming(ctx, TokenKind::LCurly)? {
        let block = block(ctx)?;
        let position = start_position.span(&block.span);

        return Ok(StatementKind::FunctionStatement {
            name: func_name,
            return_type: Some(return_type),
            param_bindings,
            statement: Some(block),
            export: exposed,
        }.statement(position))
    }

    let end_pos = eat(ctx, TokenKind::Semicolon)?.span;
    let position = start_position.span(&end_pos);

    Ok(StatementKind::FunctionStatement {
        name: func_name,
        return_type: Some(return_type),
        param_bindings,
        statement: None,
        export: exposed,
    }.statement(position))
}

fn interface_statement(ctx: &mut ParseContext) -> Result<Statement> {
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
    Ok(StatementKind::InterfaceStatement {
        name,
        functions,
    }.statement(span))
}

fn struct_statement(ctx: &mut ParseContext) -> Result<Statement> {
    let start_position = eat(ctx, TokenKind::Struct)?.span;

    let name = eat_identifier(ctx)?.0.clone();

    eat(ctx, TokenKind::LCurly)?;

    let mut fields = vec![];
    let mut implementations = vec![];

    while !is_upcoming(ctx, TokenKind::RCurly)? {
        if is_upcoming(ctx, TokenKind::Impl)? {
            let start = eat(ctx, TokenKind::Impl)?.span;

            let implements = value_expr(ctx)?;
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
            let name = eat_identifier(ctx)?.0.clone();
            let ty = type_expr(ctx)?;
            fields.push((name, ty));
            if is_upcoming(ctx, TokenKind::Comma)? {
                next(ctx)?;
            } else if !is_any_upcoming(ctx, &[TokenKind::RCurly, TokenKind::Impl])? {
                bail!("expected }} or impl")
            }
        }
    }

    let end_position = eat(ctx, TokenKind::RCurly)?.span;

    let span = start_position.span(&end_position);
    Ok(StatementKind::StructStatement {
        name,
        fields,
        implementations: vec![],
    }.statement(span))
}

fn const_statement(ctx: &mut ParseContext) -> Result<Statement> {
    let start_position = eat(ctx, TokenKind::Const)?.span;

    let name = eat_identifier(ctx)?.0.clone();
    let ty = type_expr(ctx)?;

    eat(ctx, TokenKind::Eq)?;

    let expr = value_expr(ctx)?;
    let position = start_position.span(&expr.span);

    eat(ctx, TokenKind::Semicolon)?;

    Ok(StatementKind::ConstStatement {
        name,
        const_type: ty,
        value: expr,
    }.statement(position))
}

fn definition(ctx: &mut ParseContext) -> Result<Statement> {
    let Some(peeked) = ctx.peek() else {
        bail!("early eof")
    };

    if peeked.kind == TokenKind::Struct {
        struct_statement(ctx)
    } else if peeked.kind == TokenKind::Interface {
        interface_statement(ctx)
    } else if peeked.kind == TokenKind::Const {
        const_statement(ctx)
    } else if matches!(peeked.kind, TokenKind::LParen | TokenKind::Identifier(..) | TokenKind::Export) {
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