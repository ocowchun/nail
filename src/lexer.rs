use std::fmt::format;

// lexer
pub struct Lexer {
    current_position: usize,
    peek_position: usize,
    chars: Vec<char>,
}

#[derive(PartialEq, Debug, Clone)]
pub enum TokenType {
    EOF,
    IntegerLiteral,
    FloatLiteral,
    StringLiteral,
    TimeDurationLiteral,
    Plus,
    Minus,
    Asterisk,
    Slash,
    Percent,
    Caret,
    Comma,
    LeftParenthesis,
    RightParenthesis,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Identifier,
    Equal,
    EqualEqual,
    RegexMatch,
    NotRegexMatch,
    Bang,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    And,
    Or,
    Unless,
    Offset,
    Tilde,
    Sum,
    Avg,
    Min,
    Max,
    BottomK,
    TopK,
    LimitK,
    LimitRatio,
    Group,
    Count,
    CountValues,
    Stddev,
    Stdvar,
    Quantile,
    Without,
    By,
}

#[derive(PartialEq, Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub literal: String,
}

impl Token {
    pub fn new(token_type: TokenType, literal: String) -> Token {
        Token {
            token_type,
            literal,
        }
    }

    pub fn is(&self, token_type: TokenType) -> bool {
        self.token_type == token_type
    }
}

impl Lexer {
    pub fn new(content: String) -> Lexer {
        let mut chars = vec![];
        for c in content.chars() {
            chars.push(c);
        }
        Lexer {
            current_position: 0,
            peek_position: 1,
            chars,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, String> {
        self.skip_space();

        return if let Some(c) = self.current_char() {
            match c {
                '+' => {
                    let t = Token::new(TokenType::Plus, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '-' => {
                    let t = Token::new(TokenType::Minus, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '*' => {
                    let t = Token::new(TokenType::Asterisk, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '/' => {
                    let t = Token::new(TokenType::Slash, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '%' => {
                    let t = Token::new(TokenType::Percent, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '^' => {
                    let t = Token::new(TokenType::Caret, String::from(c));
                    self.advance();
                    Ok(t)
                }
                ',' => {
                    let t = Token::new(TokenType::Comma, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '=' => {
                    let t = if let Some(c2) = self.peek_char() {
                        if c2 == '=' {
                            self.advance();
                            Token::new(TokenType::EqualEqual, String::from("=="))
                        } else if c2 == '~' {
                            self.advance();
                            Token::new(TokenType::RegexMatch, String::from("=~"))
                        } else {
                            Token::new(TokenType::Equal, String::from(c))
                        }
                    } else {
                        Token::new(TokenType::Equal, String::from(c))
                    };
                    self.advance();
                    Ok(t)
                }
                '!' => {
                    let t = if let Some(c2) = self.peek_char() {
                        if c2 == '=' {
                            self.advance();
                            Token::new(TokenType::NotEqual, String::from("!="))
                        } else if c2 == '~' {
                            self.advance();
                            Token::new(TokenType::NotRegexMatch, String::from("!~"))
                        } else {
                            Token::new(TokenType::Bang, String::from(c))
                        }
                    } else {
                        Token::new(TokenType::Bang, String::from(c))
                    };

                    self.advance();
                    Ok(t)
                }
                '>' => {
                    let t = if let Some(c2) = self.peek_char()
                        && c2 == '='
                    {
                        self.advance();
                        Token::new(TokenType::GreaterThanOrEqual, String::from(">="))
                    } else {
                        Token::new(TokenType::GreaterThan, String::from(c))
                    };

                    self.advance();
                    Ok(t)
                }
                '<' => {
                    let t = if let Some(c2) = self.peek_char()
                        && c2 == '='
                    {
                        self.advance();
                        Token::new(TokenType::LessThanOrEqual, String::from("<="))
                    } else {
                        Token::new(TokenType::LessThan, String::from(c))
                    };

                    self.advance();
                    Ok(t)
                }
                '~' => {
                    let t = Token::new(TokenType::Tilde, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '{' => {
                    let t = Token::new(TokenType::LeftBrace, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '}' => {
                    let t = Token::new(TokenType::RightBrace, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '[' => {
                    let t = Token::new(TokenType::LeftBracket, String::from(c));
                    self.advance();
                    Ok(t)
                }
                ']' => {
                    let t = Token::new(TokenType::RightBracket, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '(' => {
                    let t = Token::new(TokenType::LeftParenthesis, String::from(c));
                    self.advance();
                    Ok(t)
                }
                ')' => {
                    let t = Token::new(TokenType::RightParenthesis, String::from(c));
                    self.advance();
                    Ok(t)
                }
                '\'' => {
                    let lit = self.read_string_literal('\'')?;
                    let t = Token::new(TokenType::StringLiteral, lit);
                    Ok(t)
                }
                '"' => {
                    let lit = self.read_string_literal('"')?;
                    let t = Token::new(TokenType::StringLiteral, lit);
                    Ok(t)
                }
                '`' => {
                    let lit = self.read_string_literal('`')?;
                    let t = Token::new(TokenType::StringLiteral, lit);
                    Ok(t)
                }
                _ => {
                    if Self::is_valid_letter(c) {
                        let lit = self.read_identifier()?;
                        let t = match lit.to_lowercase().as_str() {
                            "and" => Token::new(TokenType::And, lit),
                            "or" => Token::new(TokenType::Or, lit),
                            "unless" => Token::new(TokenType::Unless, lit),
                            "offset" => Token::new(TokenType::Offset, lit),
                            "sum" => Token::new(TokenType::Sum, lit),
                            "avg" => Token::new(TokenType::Avg, lit),
                            "min" => Token::new(TokenType::Min, lit),
                            "max" => Token::new(TokenType::Max, lit),
                            "bottomk" => Token::new(TokenType::BottomK, lit),
                            "topk" => Token::new(TokenType::TopK, lit),
                            "limitk" => Token::new(TokenType::LimitK, lit),
                            "limit_ratio" => Token::new(TokenType::LimitRatio, lit),
                            "group" => Token::new(TokenType::Group, lit),
                            "count" => Token::new(TokenType::Count, lit),
                            "count_values" => Token::new(TokenType::CountValues, lit),
                            "stddev" => Token::new(TokenType::Stddev, lit),
                            "stdvar" => Token::new(TokenType::Stdvar, lit),
                            "quantile" => Token::new(TokenType::Quantile, lit),
                            "by" => Token::new(TokenType::By, lit),
                            "without" => Token::new(TokenType::Without, lit),
                            _ => Token::new(TokenType::Identifier, lit),
                        };
                        Ok(t)
                    } else if Self::is_digit(c) {
                        let t = self.read_interger_start_token()?;
                        Ok(t)
                    } else {
                        todo!()
                    }
                }
            }
        } else {
            Ok(Token::new(TokenType::EOF, "".to_owned()))
            // Err("EOF".to_string())
        };
    }

    fn advance(&mut self) {
        if self.current_position < self.chars.len() {
            self.current_position = self.peek_position;
            self.peek_position += 1;
        }
    }

    fn current_char(&self) -> Option<char> {
        if self.current_position < self.chars.len() {
            return Some(self.chars[self.current_position]);
        }
        None
    }

    fn peek_char(&self) -> Option<char> {
        if self.peek_position < self.chars.len() {
            return Some(self.chars[self.peek_position]);
        }
        None
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.current_char() {
            if !Self::is_space(c) {
                break;
            }

            self.advance();
        }
    }

    fn read_string_literal(&mut self, ending: char) -> Result<String, String> {
        let mut lit = String::new();
        self.advance();
        loop {
            if let Some(c) = self.current_char() {
                if c == ending {
                    self.advance();
                    break;
                }
                lit.push(c);
            } else {
                return Err("reach EOF when parsing string literal".to_string());
            }
            self.advance();
        }

        Ok(lit)
    }

    fn read_identifier(&mut self) -> Result<String, String> {
        let c = self.current_char().unwrap();

        let mut lit = String::from(c);
        self.advance();

        while let Some(c) = self.current_char() {
            if Self::is_digit(c) || Self::is_valid_letter(c) {
                lit.push(c);
                self.advance();
            } else {
                break;
            }
        }

        Ok(lit)
    }

    fn read_interger_start_token(&mut self) -> Result<Token, String> {
        let first = self.read_interger()?;

        if let Some(c) = self.current_char() {
            match c {
                '.' => {
                    // float: like 12.34
                    self.advance();
                    let second = self.read_interger()?;
                    return Ok(Token::new(
                        TokenType::FloatLiteral,
                        format!("{first}.{second}"),
                    ));
                }
                _ => {
                    if Self::is_valid_letter(c) {
                        let timeunit = self.read_timeunit()?;
                        return Ok(Token::new(
                            TokenType::TimeDurationLiteral,
                            format!("{first}{timeunit}"),
                        ));
                    } else if c == ')' || c == ',' || Self::is_space(c) {
                        // support case like vector(5)
                        // do nothing
                    } else {
                        return Err(format!(
                            "unexpected char `{c}` when trying to parse timeduration"
                        ));
                    }
                }
            }
        }
        return Ok(Token::new(TokenType::IntegerLiteral, first));
    }

    fn read_timeunit(&mut self) -> Result<String, String> {
        // time units: ms, s, m, h, d, w, y
        let mut lit = String::new();

        let c = self.current_char().unwrap();
        match c {
            'm' => {
                lit.push(c);
                self.advance();
                if let Some(c2) = self.current_char() {
                    if c2 == 's' {
                        lit.push(c2);
                        self.advance();
                    }
                }
            }
            's' => {
                lit.push(c);
                self.advance();
            }
            'h' => {
                lit.push(c);
                self.advance();
            }
            'd' => {
                lit.push(c);
                self.advance();
            }
            'w' => {
                lit.push(c);
                self.advance();
            }
            'y' => {
                lit.push(c);
                self.advance();
            }
            _ => {
                return Err(format!(
                    "unexpected char `{c}` when trying to parse timeunit"
                ));
            }
        }
        if let Some(c2) = self.current_char() {
            if Self::is_digit(c2) || Self::is_valid_letter(c2) {
                return Err(format!(
                    "unexpected char `{c2}` when trying to parse timeunit"
                ));
            }
        }

        Ok(lit)
    }

    fn read_interger(&mut self) -> Result<String, String> {
        let c = self.current_char().unwrap();

        let mut lit = String::from(c);
        self.advance();

        while let Some(c) = self.current_char() {
            if Self::is_digit(c) {
                lit.push(c);
                self.advance();
            } else {
                break;
            }
        }

        Ok(lit)
    }

    fn is_space(c: char) -> bool {
        return c == ' ' || c == '\n' || c == '\t' || c == '\r';
    }

    fn is_digit(c: char) -> bool {
        return c >= '0' && c <= '9';
    }

    fn is_valid_letter(c: char) -> bool {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_identifier() {
        let mut lexer = Lexer::new("http_requests_total".to_string());
        let expected_tokens = vec![Token::new(
            TokenType::Identifier,
            "http_requests_total".to_string(),
        )];

        for expected_token in expected_tokens {
            let t = lexer.next_token().unwrap();
            assert_eq!(t, expected_token);
        }
    }

    #[test]
    fn parse_operators() {
        let mut lexer = Lexer::new("== != ! = > < >= <= ^ =~ !~".to_string());
        let expected_tokens = vec![
            Token::new(TokenType::EqualEqual, "==".to_string()),
            Token::new(TokenType::NotEqual, "!=".to_string()),
            Token::new(TokenType::Bang, "!".to_string()),
            Token::new(TokenType::Equal, "=".to_string()),
            Token::new(TokenType::GreaterThan, ">".to_string()),
            Token::new(TokenType::LessThan, "<".to_string()),
            Token::new(TokenType::GreaterThanOrEqual, ">=".to_string()),
            Token::new(TokenType::LessThanOrEqual, "<=".to_string()),
            Token::new(TokenType::Caret, "^".to_string()),
            Token::new(TokenType::RegexMatch, "=~".to_string()),
            Token::new(TokenType::NotRegexMatch, "!~".to_string()),
        ];

        for expected_token in expected_tokens {
            let t = lexer.next_token().unwrap();
            assert_eq!(t, expected_token);
        }
    }

    #[test]
    fn parse_rate_expression() {
        let mut lexer = Lexer::new(
            "http_requests_total{job=\"apiserver\", handler=\"/api/comments\"}[5m]".to_string(),
        );
        let expected_tokens = vec![
            Token::new(TokenType::Identifier, "http_requests_total".to_string()),
            Token::new(TokenType::LeftBrace, "{".to_string()),
            Token::new(TokenType::Identifier, "job".to_string()),
            Token::new(TokenType::Equal, "=".to_string()),
            Token::new(TokenType::StringLiteral, "apiserver".to_string()),
            Token::new(TokenType::Comma, ",".to_string()),
            Token::new(TokenType::Identifier, "handler".to_string()),
            Token::new(TokenType::Equal, "=".to_string()),
            Token::new(TokenType::StringLiteral, "/api/comments".to_string()),
            Token::new(TokenType::RightBrace, "}".to_string()),
            Token::new(TokenType::LeftBracket, "[".to_string()),
            Token::new(TokenType::TimeDurationLiteral, "5m".to_string()),
            Token::new(TokenType::RightBracket, "]".to_string()),
        ];

        for expected_token in expected_tokens {
            let t = lexer.next_token().unwrap();
            assert_eq!(t, expected_token);
        }
    }

    #[test]
    fn parse_clamp_function() {
        let mut lexer = Lexer::new("clamp(foo, 10, 20)".to_string());
        let expected_tokens = vec![
            Token::new(TokenType::Identifier, "clamp".to_string()),
            Token::new(TokenType::LeftParenthesis, "(".to_string()),
            Token::new(TokenType::Identifier, "foo".to_string()),
            Token::new(TokenType::Comma, ",".to_string()),
            Token::new(TokenType::IntegerLiteral, "10".to_string()),
            Token::new(TokenType::Comma, ",".to_string()),
            Token::new(TokenType::IntegerLiteral, "20".to_string()),
            Token::new(TokenType::RightParenthesis, ")".to_string()),
        ];

        for expected_token in expected_tokens {
            let t = lexer.next_token().unwrap();
            assert_eq!(t, expected_token);
        }
    }
}
