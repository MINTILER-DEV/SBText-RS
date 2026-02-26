use crate::ast::Position;
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    Keyword,
    Ident,
    Number,
    String,
    Op,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub typ: TokenType,
    pub value: String,
    pub pos: Position,
}

#[derive(Debug, Clone)]
pub struct LexerError {
    pub message: String,
    pub pos: Position,
}

impl Display for LexerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (line {}, column {})",
            self.message, self.pos.line, self.pos.column
        )
    }
}

impl Error for LexerError {}

pub struct Lexer<'a> {
    chars: Vec<char>,
    index: usize,
    line: usize,
    column: usize,
    keywords: HashSet<&'static str>,
    _source: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            index: 0,
            line: 1,
            column: 1,
            keywords: keyword_set(),
            _source: source,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();
        while !self.at_end() {
            let ch = self.peek();
            if is_ignorable_format_char(ch) {
                self.advance();
                continue;
            }
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
                continue;
            }
            if ch == '\n' {
                let pos = self.pos();
                self.advance();
                tokens.push(Token {
                    typ: TokenType::Newline,
                    value: "\n".to_string(),
                    pos,
                });
                continue;
            }
            if ch == '#' {
                self.skip_comment();
                continue;
            }
            if ch == '"' {
                tokens.push(self.read_string()?);
                continue;
            }
            if ch.is_ascii_digit() {
                tokens.push(self.read_number());
                continue;
            }
            if ch.is_ascii_alphabetic() || ch == '_' {
                tokens.push(self.read_identifier());
                continue;
            }
            let pos = self.pos();
            match ch {
                '(' => {
                    self.advance();
                    tokens.push(Token {
                        typ: TokenType::LParen,
                        value: "(".to_string(),
                        pos,
                    });
                }
                ')' => {
                    self.advance();
                    tokens.push(Token {
                        typ: TokenType::RParen,
                        value: ")".to_string(),
                        pos,
                    });
                }
                '[' => {
                    self.advance();
                    tokens.push(Token {
                        typ: TokenType::LBracket,
                        value: "[".to_string(),
                        pos,
                    });
                }
                ']' => {
                    self.advance();
                    tokens.push(Token {
                        typ: TokenType::RBracket,
                        value: "]".to_string(),
                        pos,
                    });
                }
                ',' => {
                    self.advance();
                    tokens.push(Token {
                        typ: TokenType::Comma,
                        value: ",".to_string(),
                        pos,
                    });
                }
                '+' | '-' | '*' | '/' | '%' => {
                    self.advance();
                    tokens.push(Token {
                        typ: TokenType::Op,
                        value: ch.to_string(),
                        pos,
                    });
                }
                '=' | '!' | '<' | '>' => {
                    tokens.push(self.read_operator());
                }
                _ => {
                    return Err(LexerError {
                        message: format!("Unexpected character {:?}", ch),
                        pos,
                    });
                }
            }
        }
        tokens.push(Token {
            typ: TokenType::Eof,
            value: String::new(),
            pos: self.pos(),
        });
        Ok(tokens)
    }

    fn read_operator(&mut self) -> Token {
        let pos = self.pos();
        let ch = self.advance();
        let mut value = ch.to_string();
        if matches!(ch, '=' | '!' | '<' | '>') && self.peek() == '=' {
            value.push(self.advance());
        }
        Token {
            typ: TokenType::Op,
            value,
            pos,
        }
    }

    fn read_identifier(&mut self) -> Token {
        let pos = self.pos();
        let mut text = String::new();
        text.push(self.advance());
        while !self.at_end() {
            let ch = self.peek();
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '?' {
                text.push(self.advance());
            } else if ch == '.' {
                text.push(self.advance());
            } else {
                break;
            }
        }
        let lowered = text.to_lowercase();
        if self.keywords.contains(lowered.as_str()) {
            Token {
                typ: TokenType::Keyword,
                value: lowered,
                pos,
            }
        } else {
            Token {
                typ: TokenType::Ident,
                value: text,
                pos,
            }
        }
    }

    fn read_number(&mut self) -> Token {
        let pos = self.pos();
        let mut text = String::new();
        text.push(self.advance());

        if text == "0" && !self.at_end() {
            let radix_prefix = self.peek();
            if matches!(radix_prefix, 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
                text.push(self.advance());
                while !self.at_end() {
                    let ch = self.peek();
                    let is_valid = match radix_prefix {
                        'x' | 'X' => ch.is_ascii_hexdigit(),
                        'b' | 'B' => matches!(ch, '0' | '1'),
                        'o' | 'O' => matches!(ch, '0'..='7'),
                        _ => false,
                    };
                    if is_valid || ch == '_' {
                        text.push(self.advance());
                    } else {
                        break;
                    }
                }
                return Token {
                    typ: TokenType::Number,
                    value: text,
                    pos,
                };
            }
        }

        let mut seen_dot = false;
        while !self.at_end() {
            let ch = self.peek();
            if ch.is_ascii_digit() {
                text.push(self.advance());
                continue;
            }
            if ch == '.' && !seen_dot {
                seen_dot = true;
                text.push(self.advance());
                continue;
            }
            break;
        }
        Token {
            typ: TokenType::Number,
            value: text,
            pos,
        }
    }

    fn read_string(&mut self) -> Result<Token, LexerError> {
        let pos = self.pos();
        self.advance();
        let mut out = String::new();
        while !self.at_end() {
            let ch = self.advance();
            if ch == '"' {
                return Ok(Token {
                    typ: TokenType::String,
                    value: out,
                    pos,
                });
            }
            if ch == '\\' {
                if self.at_end() {
                    break;
                }
                let esc = self.advance();
                let mapped = match esc {
                    '"' => '"',
                    '\\' => '\\',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => esc,
                };
                out.push(mapped);
                continue;
            }
            if ch == '\n' {
                return Err(LexerError {
                    message: "Unterminated string literal".to_string(),
                    pos,
                });
            }
            out.push(ch);
        }
        Err(LexerError {
            message: "Unterminated string literal".to_string(),
            pos,
        })
    }

    fn skip_comment(&mut self) {
        while !self.at_end() && self.peek() != '\n' {
            self.advance();
        }
    }

    fn at_end(&self) -> bool {
        self.index >= self.chars.len()
    }

    fn peek(&self) -> char {
        if self.at_end() {
            '\0'
        } else {
            self.chars[self.index]
        }
    }

    fn advance(&mut self) -> char {
        let ch = self.chars[self.index];
        self.index += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn pos(&self) -> Position {
        Position::new(self.line, self.column)
    }
}

fn keyword_set() -> HashSet<&'static str> {
    [
        "add",
        "all",
        "and",
        "answer",
        "ask",
        "at",
        "backdrop",
        "bounce",
        "broadcast",
        "brightness",
        "by",
        "change",
        "clicked",
        "color",
        "contains",
        "contents",
        "costume",
        "down",
        "define",
        "delete",
        "direction",
        "edge",
        "else",
        "end",
        "erase",
        "each",
        "flag",
        "floor",
        "for",
        "forever",
        "go",
        "hide",
        "i",
        "if",
        "in",
        "insert",
        "item",
        "key",
        "left",
        "length",
        "list",
        "mouse",
        "move",
        "next",
        "not",
        "of",
        "on",
        "or",
        "pick",
        "point",
        "pressed",
        "random",
        "receive",
        "repeat",
        "replace",
        "reset",
        "right",
        "round",
        "say",
        "saturation",
        "seconds",
        "set",
        "show",
        "size",
        "sprite",
        "stamp",
        "stage",
        "steps",
        "stop",
        "switch",
        "pen",
        "then",
        "think",
        "this",
        "timer",
        "to",
        "transparency",
        "turn",
        "up",
        "until",
        "var",
        "wait",
        "while",
        "when",
        "with",
        "x",
        "y",
    ]
    .into_iter()
    .collect()
}

fn is_ignorable_format_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{feff}' // BOM / zero width no-break space
            | '\u{200b}' // zero width space
            | '\u{200c}' // zero width non-joiner
            | '\u{200d}' // zero width joiner
            | '\u{2060}' // word joiner
    )
}
