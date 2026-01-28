//! Selector language parser and AST.
//!
//! Parses expressions like:
//! - ws(active)
//! - ws(active) & ahead("main")
//! - ws(name~"agent") ~ ws(blocked)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorExpr {
    Atom(SelectorAtom),
    Union(Box<SelectorExpr>, Box<SelectorExpr>),
    Intersection(Box<SelectorExpr>, Box<SelectorExpr>),
    Difference(Box<SelectorExpr>, Box<SelectorExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorAtom {
    Entity(EntitySelector),
    Predicate(Predicate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySelector {
    pub kind: EntityKind,
    pub predicate: Option<Predicate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Workspace,
    Lease,
    Branch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    Active,
    Stale,
    Blocked,
    NameMatches(String),
    Ahead(String),
    Touching(String),
    Overlaps(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectorItem {
    pub id: String,
    pub name: String,
}

impl SelectorItem {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectorMatch {
    pub kind: EntityKind,
    pub item: SelectorItem,
}

impl SelectorMatch {
    pub fn new(kind: EntityKind, item: SelectorItem) -> Self {
        Self { kind, item }
    }
}

pub struct SelectorContext<'a, F>
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    pub workspaces: &'a [SelectorItem],
    pub leases: &'a [SelectorItem],
    pub branches: &'a [SelectorItem],
    pub matches: F,
}

impl<'a, F> SelectorContext<'a, F>
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    pub fn new(
        workspaces: &'a [SelectorItem],
        leases: &'a [SelectorItem],
        branches: &'a [SelectorItem],
        matches: F,
    ) -> Self {
        Self {
            workspaces,
            leases,
            branches,
            matches,
        }
    }
}

pub fn evaluate_selector<F>(expr: &SelectorExpr, ctx: &SelectorContext<F>) -> Vec<SelectorMatch>
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    let mut values: Vec<SelectorMatch> = eval_expr(expr, ctx).into_iter().collect();
    values.sort_by(|a, b| {
        let rank_a = kind_rank(a.kind);
        let rank_b = kind_rank(b.kind);
        rank_a.cmp(&rank_b).then_with(|| a.item.id.cmp(&b.item.id))
    });
    values
}

fn eval_expr<F>(
    expr: &SelectorExpr,
    ctx: &SelectorContext<F>,
) -> std::collections::HashSet<SelectorMatch>
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    use std::collections::HashSet;

    match expr {
        SelectorExpr::Atom(atom) => eval_atom(atom, ctx),
        SelectorExpr::Union(left, right) => {
            let mut out = eval_expr(left, ctx);
            out.extend(eval_expr(right, ctx));
            out
        }
        SelectorExpr::Intersection(left, right) => {
            let left_set = eval_expr(left, ctx);
            let right_set = eval_expr(right, ctx);
            left_set
                .intersection(&right_set)
                .cloned()
                .collect::<HashSet<_>>()
        }
        SelectorExpr::Difference(left, right) => {
            let left_set = eval_expr(left, ctx);
            let right_set = eval_expr(right, ctx);
            left_set
                .difference(&right_set)
                .cloned()
                .collect::<HashSet<_>>()
        }
    }
}

fn eval_atom<F>(
    atom: &SelectorAtom,
    ctx: &SelectorContext<F>,
) -> std::collections::HashSet<SelectorMatch>
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    use std::collections::HashSet;

    match atom {
        SelectorAtom::Entity(entity) => eval_entity(entity, ctx),
        SelectorAtom::Predicate(predicate) => {
            let mut out = HashSet::new();
            for kind in [EntityKind::Workspace, EntityKind::Lease, EntityKind::Branch] {
                for item in items_for_kind(ctx, kind) {
                    if predicate_matches(ctx, kind, item, predicate) {
                        out.insert(SelectorMatch::new(kind, item.clone()));
                    }
                }
            }
            out
        }
    }
}

fn eval_entity<F>(
    entity: &EntitySelector,
    ctx: &SelectorContext<F>,
) -> std::collections::HashSet<SelectorMatch>
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    let mut out = std::collections::HashSet::new();
    let kind = entity.kind;
    for item in items_for_kind(ctx, kind) {
        let matches = match &entity.predicate {
            Some(predicate) => predicate_matches(ctx, kind, item, predicate),
            None => true,
        };
        if matches {
            out.insert(SelectorMatch::new(kind, item.clone()));
        }
    }
    out
}

fn items_for_kind<'a, F>(ctx: &'a SelectorContext<F>, kind: EntityKind) -> &'a [SelectorItem]
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    match kind {
        EntityKind::Workspace => ctx.workspaces,
        EntityKind::Lease => ctx.leases,
        EntityKind::Branch => ctx.branches,
    }
}

fn predicate_matches<F>(
    ctx: &SelectorContext<F>,
    kind: EntityKind,
    item: &SelectorItem,
    predicate: &Predicate,
) -> bool
where
    F: Fn(EntityKind, &SelectorItem, &Predicate) -> bool,
{
    match predicate {
        Predicate::NameMatches(pattern) => item.name.contains(pattern),
        _ => (ctx.matches)(kind, item, predicate),
    }
}

fn kind_rank(kind: EntityKind) -> u8 {
    match kind {
        EntityKind::Workspace => 0,
        EntityKind::Lease => 1,
        EntityKind::Branch => 2,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for SelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.position)
    }
}

impl std::error::Error for SelectorError {}

pub fn parse_selector(input: &str) -> Result<SelectorExpr, SelectorError> {
    let mut parser = Parser::new(input)?;
    let expr = parser.parse_expr()?;
    parser.expect(TokenKind::Eof)?;
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Str(String),
    LParen,
    RParen,
    Pipe,
    Amp,
    Tilde,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    pos: usize,
}

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn next_token(&mut self) -> Result<Token, SelectorError> {
        self.skip_ws();
        let pos = self.pos;
        if self.pos >= self.input.len() {
            return Ok(Token {
                kind: TokenKind::Eof,
                pos,
            });
        }
        let ch = self.input[self.pos] as char;
        match ch {
            '(' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::LParen,
                    pos,
                })
            }
            ')' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::RParen,
                    pos,
                })
            }
            '|' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Pipe,
                    pos,
                })
            }
            '&' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Amp,
                    pos,
                })
            }
            '~' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Tilde,
                    pos,
                })
            }
            '"' => self.read_string(),
            _ if is_ident_start(ch) => self.read_ident(),
            _ => Err(SelectorError {
                message: format!("Unexpected character '{ch}'"),
                position: pos,
            }),
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos] as char;
            if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn read_ident(&mut self) -> Result<Token, SelectorError> {
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.input.len() {
            let ch = self.input[self.pos] as char;
            if is_ident_continue(ch) {
                self.pos += 1;
            } else {
                break;
            }
        }
        let ident = std::str::from_utf8(&self.input[start..self.pos]).unwrap();
        Ok(Token {
            kind: TokenKind::Ident(ident.to_string()),
            pos: start,
        })
    }

    fn read_string(&mut self) -> Result<Token, SelectorError> {
        let start = self.pos;
        self.pos += 1; // consume opening quote
        let mut out = String::new();
        while self.pos < self.input.len() {
            let ch = self.input[self.pos] as char;
            match ch {
                '"' => {
                    self.pos += 1;
                    return Ok(Token {
                        kind: TokenKind::Str(out),
                        pos: start,
                    });
                }
                '\\' => {
                    self.pos += 1;
                    if self.pos >= self.input.len() {
                        break;
                    }
                    let esc = self.input[self.pos] as char;
                    let mapped = match esc {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    };
                    out.push(mapped);
                    self.pos += 1;
                }
                _ => {
                    out.push(ch);
                    self.pos += 1;
                }
            }
        }
        Err(SelectorError {
            message: "Unterminated string literal".to_string(),
            position: start,
        })
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

struct Parser<'a> {
    tokens: Vec<Token>,
    index: usize,
    _input: &'a str,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Result<Self, SelectorError> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token()?;
            let is_eof = matches!(token.kind, TokenKind::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(Self {
            tokens,
            index: 0,
            _input: input,
        })
    }

    fn parse_expr(&mut self) -> Result<SelectorExpr, SelectorError> {
        self.parse_union()
    }

    fn parse_union(&mut self) -> Result<SelectorExpr, SelectorError> {
        let mut expr = self.parse_intersection()?;
        while self.peek_is(&TokenKind::Pipe) {
            self.next_token();
            let rhs = self.parse_intersection()?;
            expr = SelectorExpr::Union(Box::new(expr), Box::new(rhs));
        }
        Ok(expr)
    }

    fn parse_intersection(&mut self) -> Result<SelectorExpr, SelectorError> {
        let mut expr = self.parse_term()?;
        loop {
            if self.peek_is(&TokenKind::Amp) {
                self.next_token();
                let rhs = self.parse_term()?;
                expr = SelectorExpr::Intersection(Box::new(expr), Box::new(rhs));
            } else if self.peek_is(&TokenKind::Tilde) {
                self.next_token();
                let rhs = self.parse_term()?;
                expr = SelectorExpr::Difference(Box::new(expr), Box::new(rhs));
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<SelectorExpr, SelectorError> {
        match self.peek_kind() {
            TokenKind::LParen => {
                self.next_token();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Ident(_) => {
                let atom = self.parse_atom()?;
                Ok(SelectorExpr::Atom(atom))
            }
            _ => Err(self.error_here("Expected selector term")),
        }
    }

    fn parse_atom(&mut self) -> Result<SelectorAtom, SelectorError> {
        let ident = self.expect_ident()?;
        if self.peek_is(&TokenKind::LParen) {
            if let Some(kind) = parse_entity_kind(&ident) {
                self.next_token(); // consume '('
                let predicate = if self.peek_is(&TokenKind::RParen) {
                    None
                } else {
                    Some(self.parse_predicate()?)
                };
                self.expect(TokenKind::RParen)?;
                Ok(SelectorAtom::Entity(EntitySelector { kind, predicate }))
            } else {
                let predicate = self.parse_predicate_from_ident(ident)?;
                Ok(SelectorAtom::Predicate(predicate))
            }
        } else {
            let predicate = self.parse_predicate_from_ident(ident)?;
            Ok(SelectorAtom::Predicate(predicate))
        }
    }

    fn parse_predicate(&mut self) -> Result<Predicate, SelectorError> {
        let ident = self.expect_ident()?;
        self.parse_predicate_from_ident(ident)
    }

    fn parse_predicate_from_ident(&mut self, ident: String) -> Result<Predicate, SelectorError> {
        match ident.as_str() {
            "active" => Ok(Predicate::Active),
            "stale" => Ok(Predicate::Stale),
            "blocked" => Ok(Predicate::Blocked),
            "name" => {
                self.expect(TokenKind::Tilde)?;
                let value = self.expect_string()?;
                Ok(Predicate::NameMatches(value))
            }
            "ahead" => Ok(Predicate::Ahead(self.parse_call_arg()?)),
            "touching" => Ok(Predicate::Touching(self.parse_call_arg()?)),
            "overlaps" => Ok(Predicate::Overlaps(self.parse_call_arg()?)),
            _ => Err(self.error_here(&format!("Unknown predicate '{ident}'"))),
        }
    }

    fn parse_call_arg(&mut self) -> Result<String, SelectorError> {
        self.expect(TokenKind::LParen)?;
        let value = self.expect_string()?;
        self.expect(TokenKind::RParen)?;
        Ok(value)
    }

    fn expect_string(&mut self) -> Result<String, SelectorError> {
        match self.next_token().kind {
            TokenKind::Str(value) => Ok(value),
            _ => Err(self.error_here("Expected string literal")),
        }
    }

    fn expect_ident(&mut self) -> Result<String, SelectorError> {
        match self.next_token().kind {
            TokenKind::Ident(value) => Ok(value),
            _ => Err(self.error_here("Expected identifier")),
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), SelectorError> {
        let token = self.next_token();
        if std::mem::discriminant(&token.kind) == std::mem::discriminant(&kind) {
            Ok(())
        } else {
            Err(self.error_here(&format!("Expected {:?}", kind_discriminant_name(&kind))))
        }
    }

    fn error_here(&self, message: &str) -> SelectorError {
        let pos = self.tokens.get(self.index).map(|t| t.pos).unwrap_or(0);
        SelectorError {
            message: message.to_string(),
            position: pos,
        }
    }

    fn next_token(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        self.index = (self.index + 1).min(self.tokens.len() - 1);
        token
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.index].kind
    }

    fn peek_is(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }
}

fn parse_entity_kind(ident: &str) -> Option<EntityKind> {
    match ident {
        "ws" => Some(EntityKind::Workspace),
        "lease" => Some(EntityKind::Lease),
        "branch" => Some(EntityKind::Branch),
        _ => None,
    }
}

fn kind_discriminant_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Ident(_) => "identifier",
        TokenKind::Str(_) => "string",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::Pipe => "|",
        TokenKind::Amp => "&",
        TokenKind::Tilde => "~",
        TokenKind::Eof => "end of input",
    }
}
