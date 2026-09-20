use std::mem;

use crate::ast::AggregationExpression::Simple;
use crate::ast::BinaryExpression;
use crate::ast::BinaryOperator;
use crate::ast::CallExpression;
use crate::ast::Expression;
use crate::ast::Expression::AggregationExpression;
use crate::ast::FloatLiteral;
use crate::ast::LabelMatcher;
use crate::ast::LabelMatcherOperator;
use crate::ast::SimpleAggregationExpression;
use crate::ast::SimpleAggregationOperator;
use crate::ast::TimeDuration;
use crate::ast::TimeSeries;
use crate::lexer::Lexer;

use crate::lexer::Token;
use crate::lexer::TokenType;
use crate::lexer::TokenType::LeftBrace;
use crate::lexer::TokenType::TimeDurationLiteral;

pub struct Parser {
    lexer: Lexer,
    current_token: Token,
    peek_token: Token,
    error: Option<String>,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Result<Parser, String> {
        let mut p = Parser {
            lexer,
            current_token: Token::new(TokenType::EOF, "".to_string()),
            peek_token: Token::new(TokenType::EOF, "".to_string()),
            error: None,
        };
        p.next_token()?;
        p.next_token()?;

        Ok(p)
    }

    fn next_token(&mut self) -> Result<Option<Token>, String> {
        if let Some(_) = &self.error {
            return Ok(None);
        }

        match self.lexer.next_token() {
            Ok(mut token) => {
                mem::swap(&mut self.peek_token, &mut token);
                mem::swap(&mut self.current_token, &mut token);
                return Ok(Some(token));
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    pub fn parse(&mut self) -> Result<Expression, String> {
        if let Some(err) = &self.error {
            return Err(err.to_string());
        }

        self.parse_expression()
    }

    fn parse_expression(&mut self) -> Result<Expression, String> {
        if Self::is_aggregation_operator_token(&self.current_token) {
            self.parse_aggregation()
        } else {
            self.parse_or()
        }
    }

    fn is_aggregation_operator_token(token: &Token) -> bool {
        vec![
            TokenType::Sum,
            TokenType::Avg,
            TokenType::Min,
            TokenType::Max,
            TokenType::BottomK,
            TokenType::TopK,
            TokenType::LimitK,
            TokenType::LimitRatio,
            TokenType::Group,
            TokenType::Count,
            TokenType::CountValues,
            TokenType::Stddev,
            TokenType::Stdvar,
            TokenType::Quantile,
        ]
        .contains(&token.token_type)
    }

    fn parse_aggregation(&mut self) -> Result<Expression, String> {
        let aggregation_operator_token = self.next_token()?.unwrap();
        match aggregation_operator_token.token_type {
            TokenType::Sum
            | TokenType::Avg
            | TokenType::Min
            | TokenType::Max
            | TokenType::Group
            | TokenType::Count
            | TokenType::Stddev
            | TokenType::Stdvar => {
                self.parse_simple_aggregation(aggregation_operator_token.token_type)
            }
            _ => return Err("not implement yet".to_string()),
        }
    }

    fn parse_simple_aggregation(&mut self, token_type: TokenType) -> Result<Expression, String> {
        let op = match token_type {
            TokenType::Sum => SimpleAggregationOperator::Sum,
            TokenType::Avg => SimpleAggregationOperator::Avg,
            TokenType::Min => SimpleAggregationOperator::Min,
            TokenType::Max => SimpleAggregationOperator::Max,
            TokenType::Group => SimpleAggregationOperator::Group,
            TokenType::Count => SimpleAggregationOperator::Count,
            TokenType::Stddev => SimpleAggregationOperator::Stddev,
            TokenType::Stdvar => SimpleAggregationOperator::Stdvar,
            _ => return Err("invalid aggregation operator".to_string()),
        };

        let mut is_without = false;
        let mut labels =
            if vec![TokenType::By, TokenType::Without].contains(&self.current_token.token_type) {
                is_without = self.current_token.is(TokenType::Without);
                self.next_token()?;
                self.parse_label_list()?
            } else {
                vec![]
            };

        if !self.current_token.is(TokenType::LeftParenthesis) {
            return Err(format!(
                "expected `(` when parsing aggregation expression got {}",
                self.current_token.literal
            ));
        }
        self.next_token()?;

        let exp = self.parse()?;

        if !self.current_token.is(TokenType::RightParenthesis) {
            return Err(format!(
                "expected `)` when parsing aggregation expression got {}",
                self.current_token.literal
            ));
        }
        self.next_token()?;

        if vec![TokenType::By, TokenType::Without].contains(&self.current_token.token_type) {
            if !labels.is_empty() {
                return Err("double without/by clause".to_string());
            }
            is_without = self.current_token.is(TokenType::Without);
            self.next_token()?;
            labels = self.parse_label_list()?;
        }

        let exp = SimpleAggregationExpression::new(op, Box::new(exp), labels, is_without);
        Ok(AggregationExpression(Simple(exp)))
    }

    fn parse_label_list(&mut self) -> Result<Vec<String>, String> {
        let mut labels = vec![];
        if !self.current_token.is(TokenType::LeftParenthesis) {
            return Err(format!(
                "expected `(` when parsing aggregation expression got {}",
                self.current_token.literal
            ));
        }
        self.next_token()?;

        while !self.current_token.is(TokenType::RightParenthesis) {
            if !self.current_token.is(TokenType::Identifier) {
                return Err(format!(
                    "expected identifier when parsing aggregation expression got {}",
                    self.current_token.literal
                ));
            }

            let token = self.next_token()?.unwrap();
            labels.push(token.literal);
            if self.current_token.is(TokenType::Comma) {
                self.next_token()?;
            }
        }
        self.next_token()?;

        if labels.is_empty() {
            return Err(format!("it must contains at least one label"));
        }

        Ok(labels)
    }

    fn parse_or(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_and_unless()?;

        loop {
            let op = match self.current_token.token_type {
                TokenType::Or => BinaryOperator::Or,
                _ => {
                    break;
                }
            };
            self.next_token()?;

            let right = self.parse_and_unless()?;

            left = Expression::BinaryExpression(BinaryExpression::new(
                op,
                Box::new(left),
                Box::new(right),
            ));
        }

        Ok(left)
    }

    fn parse_and_unless(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_comparison()?;

        loop {
            let op = match self.current_token.token_type {
                TokenType::And => BinaryOperator::And,
                TokenType::Unless => BinaryOperator::Unless,
                _ => {
                    break;
                }
            };
            self.next_token()?;

            let right = self.parse_comparison()?;

            left = Expression::BinaryExpression(BinaryExpression::new(
                op,
                Box::new(left),
                Box::new(right),
            ));
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_term()?;

        loop {
            let op = match self.current_token.token_type {
                TokenType::EqualEqual => BinaryOperator::Equal,
                TokenType::NotEqual => BinaryOperator::NotEqual,
                TokenType::LessThanOrEqual => BinaryOperator::LessThanOrEqual,
                TokenType::LessThan => BinaryOperator::LessThan,
                TokenType::GreaterThanOrEqual => BinaryOperator::GreaterThanOrEqual,
                TokenType::GreaterThan => BinaryOperator::GreaterThan,
                _ => {
                    break;
                }
            };
            self.next_token()?;

            let right = self.parse_term()?;

            left = Expression::BinaryExpression(BinaryExpression::new(
                op,
                Box::new(left),
                Box::new(right),
            ));
        }

        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_factor()?;

        loop {
            let op = match self.current_token.token_type {
                TokenType::Plus => BinaryOperator::Plus,
                TokenType::Minus => BinaryOperator::Minus,
                _ => {
                    break;
                }
            };
            self.next_token()?;

            let right = self.parse_factor()?;

            left = Expression::BinaryExpression(BinaryExpression::new(
                op,
                Box::new(left),
                Box::new(right),
            ));
        }

        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_caret()?;

        loop {
            let op = match self.current_token.token_type {
                TokenType::Asterisk => BinaryOperator::Multiply,
                TokenType::Slash => BinaryOperator::Divide,
                TokenType::Percent => BinaryOperator::Mod,
                _ => {
                    break;
                }
            };
            self.next_token()?;

            let right = self.parse_caret()?;

            left = Expression::BinaryExpression(BinaryExpression::new(
                op,
                Box::new(left),
                Box::new(right),
            ));
        }

        Ok(left)
    }

    fn parse_caret(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_primary()?;

        loop {
            let op = match self.current_token.token_type {
                TokenType::Caret => BinaryOperator::Power,
                _ => {
                    break;
                }
            };
            self.next_token()?;

            let right = self.parse_primary()?;

            left = Expression::BinaryExpression(BinaryExpression::new(
                op,
                Box::new(left),
                Box::new(right),
            ));
        }

        Ok(left)
    }

    fn finish_call(&mut self, callee: String) -> Result<Expression, String> {
        let mut arguments = vec![];

        while !self.current_token.is(TokenType::RightParenthesis) {
            if !arguments.is_empty() {
                if !self.current_token.is(TokenType::Comma) {
                    return Err(format!(
                        "expected `,` when parsing call got {}",
                        self.current_token.literal
                    ));
                }
                self.next_token()?;
            }

            let arg = self.parse_expression()?;
            arguments.push(Box::new(arg));
        }

        self.next_token()?;

        let call = CallExpression::new(callee, arguments);
        Ok(Expression::CallExpression(call))
    }

    fn parse_primary(&mut self) -> Result<Expression, String> {
        if self.current_token.is(TokenType::IntegerLiteral)
            || self.current_token.is(TokenType::FloatLiteral)
        {
            let token = self.next_token()?.unwrap();
            let lit = FloatLiteral::new(token.literal);
            return Ok(Expression::FloatLiteral(lit));
        }
        if self.current_token.is(TokenType::StringLiteral) {
            let token = self.next_token()?.unwrap();
            let lit = token.literal;

            return Ok(Expression::StringLiteral(lit));
        }

        if self.current_token.is(TokenType::LeftParenthesis) {
            self.next_token()?;
            let exp = self.parse_expression()?;
            if self.current_token.is(TokenType::RightParenthesis) {
                self.next_token()?;
                // TODO: might need a new expression type?
                return Ok(exp);
            } else {
                return Err(format!("expected `)` got {}", self.current_token.literal,));
            }
        }

        if self.current_token.is(TokenType::Identifier) {
            if self.peek_token.is(TokenType::LeftParenthesis) {
                let token = self.next_token()?.unwrap();
                self.next_token()?;
                return self.finish_call(token.literal);
            }
            return self.parse_timeseries();
        }

        Err(format!(
            "failed to parse primary from `{}`",
            self.current_token.literal,
        ))
    }

    fn parse_timeseries(&mut self) -> Result<Expression, String> {
        let name = self.next_token()?.unwrap().literal;
        let mut label_matchers = vec![];

        if self.current_token.is(LeftBrace) {
            self.next_token()?;
            while !self.current_token.is(TokenType::RightBrace) {
                if !label_matchers.is_empty() {
                    if !self.current_token.is(TokenType::Comma) {
                        return Err(format!(
                            "expect , when parsing label_matcher but got `{}`",
                            self.current_token.literal,
                        ));
                    }
                    self.next_token()?;
                }

                if !self.current_token.is(TokenType::Identifier) {
                    return Err(format!(
                        "expect identifier when parsing label_matcher but got `{}`",
                        self.current_token.literal,
                    ));
                }
                let label_name = self.next_token()?.unwrap().literal;
                let op = match self.current_token.token_type {
                    TokenType::Equal => LabelMatcherOperator::Equal,
                    TokenType::NotEqual => LabelMatcherOperator::NotEqual,
                    TokenType::RegexMatch => LabelMatcherOperator::RegexMatch,
                    TokenType::NotRegexMatch => LabelMatcherOperator::NotRegexMatch,
                    _ => {
                        return Err(format!(
                            "expect label matcher operator when parsing label_matcher but got `{}`",
                            self.current_token.literal,
                        ));
                    }
                };
                self.next_token()?;

                if !self.current_token.is(TokenType::StringLiteral) {
                    return Err(format!(
                        "expect string literal when parsing label_matcher but got `{}`",
                        self.current_token.literal,
                    ));
                }
                let label_value = self.next_token()?.unwrap().literal;
                let matcher = LabelMatcher::new(op, label_name, label_value);
                label_matchers.push(matcher);
            }

            self.next_token()?;
        }

        let range = if self.current_token.is(TokenType::LeftBracket) {
            self.next_token()?;
            if !self.current_token.is(TimeDurationLiteral) {
                return Err(format!(
                    "expect time duration literal when parsing range but got `{}`",
                    self.current_token.literal,
                ));
            }

            let token = self.next_token()?.unwrap();
            let duration = TimeDuration::from(token.literal)?;
            if !self.current_token.is(TokenType::RightBracket) {
                return Err(format!(
                    "expect `]` when parsing range but got `{}`",
                    self.current_token.literal,
                ));
            }
            self.next_token()?;
            Some(duration)
        } else {
            None
        };

        let offset = if self.current_token.is(TokenType::Offset) {
            self.next_token()?;
            let token = self.next_token()?.unwrap();
            let duration = TimeDuration::from(token.literal)?;
            Some(duration)
        } else {
            None
        };

        let exp = TimeSeries::new(name, label_matchers, range, offset);
        Ok(Expression::TimeSeries(exp))
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::TimeUnit::Minute;

    use super::*;

    #[test]
    fn parse_simple_expression() {
        let lexer = Lexer::new("http_requests_total".to_string());
        let mut parser = Parser::new(lexer).unwrap();

        let exp = parser.parse().unwrap();
        let expected_exp = Expression::TimeSeries(TimeSeries::new(
            "http_requests_total".to_string(),
            vec![],
            None,
            None,
        ));

        assert_eq!(exp, expected_exp);
    }

    #[test]
    fn parse_timeseries() {
        let series = vec![
            (
                "http_requests_total",
                Expression::TimeSeries(TimeSeries::new(
                    "http_requests_total".to_string(),
                    vec![],
                    None,
                    None,
                )),
            ),
            (
                "http_requests_total{replica=\"rep-a\"}",
                Expression::TimeSeries(TimeSeries::new(
                    "http_requests_total".to_string(),
                    vec![LabelMatcher::new(
                        LabelMatcherOperator::Equal,
                        "replica".to_string(),
                        "rep-a".to_string(),
                    )],
                    None,
                    None,
                )),
            ),
            (
                "http_requests_total{job=\"prometheus\"}[5m]",
                Expression::TimeSeries(TimeSeries::new(
                    "http_requests_total".to_string(),
                    vec![LabelMatcher::new(
                        LabelMatcherOperator::Equal,
                        "job".to_string(),
                        "prometheus".to_string(),
                    )],
                    Some(TimeDuration::from("5m".to_string()).unwrap()),
                    None,
                )),
            ),
            (
                "http_requests_total offset 5m",
                Expression::TimeSeries(TimeSeries::new(
                    "http_requests_total".to_string(),
                    vec![],
                    None,
                    Some(TimeDuration::from("5m".to_string()).unwrap()),
                )),
            ),
            (
                "http_requests_total[5m] offset 1w",
                Expression::TimeSeries(TimeSeries::new(
                    "http_requests_total".to_string(),
                    vec![],
                    Some(TimeDuration::from("5m".to_string()).unwrap()),
                    Some(TimeDuration::from("1w".to_string()).unwrap()),
                )),
            ),
            (
                "http_requests_total{method=\"GET\"} offset 5m",
                Expression::TimeSeries(TimeSeries::new(
                    "http_requests_total".to_string(),
                    vec![LabelMatcher::new(
                        LabelMatcherOperator::Equal,
                        "method".to_string(),
                        "GET".to_string(),
                    )],
                    None,
                    Some(TimeDuration::from("5m".to_string()).unwrap()),
                )),
            ),
        ];
        series.into_iter().for_each(|(query, expected_exp)| {
            let lexer = Lexer::new(query.to_string());
            let mut parser = Parser::new(lexer).unwrap();
            let exp = parser.parse().unwrap();

            assert_eq!(exp, expected_exp);
        });
    }

    #[test]
    fn parse_or_operator() {
        let lexer = Lexer::new("foo or bar".to_string());
        let mut parser = Parser::new(lexer).unwrap();

        let exp = parser.parse().unwrap();
        let expected_exp = Expression::BinaryExpression(BinaryExpression::new(
            BinaryOperator::Or,
            Box::new(Expression::TimeSeries(TimeSeries::new(
                "foo".to_string(),
                vec![],
                None,
                None,
            ))),
            Box::new(Expression::TimeSeries(TimeSeries::new(
                "bar".to_string(),
                vec![],
                None,
                None,
            ))),
        ));

        assert_eq!(exp, expected_exp);
    }

    #[test]
    fn parse_binary_operator() {
        let operators = vec![
            ("and", BinaryOperator::And),
            ("or", BinaryOperator::Or),
            ("unless", BinaryOperator::Unless),
            ("==", BinaryOperator::Equal),
            ("!=", BinaryOperator::NotEqual),
            (">", BinaryOperator::GreaterThan),
            ("<", BinaryOperator::LessThan),
            ("+", BinaryOperator::Plus),
            ("-", BinaryOperator::Minus),
            ("*", BinaryOperator::Multiply),
            ("/", BinaryOperator::Divide),
            ("%", BinaryOperator::Mod),
            ("^", BinaryOperator::Power),
        ];
        operators.iter().for_each(|(op_str, op)| {
            let query = format!("foo {op_str} bar");
            let lexer = Lexer::new(query);
            let mut parser = Parser::new(lexer).unwrap();

            let exp = parser.parse().unwrap();
            let expected_exp = Expression::BinaryExpression(BinaryExpression::new(
                op.clone(),
                Box::new(Expression::TimeSeries(TimeSeries::new(
                    "foo".to_string(),
                    vec![],
                    None,
                    None,
                ))),
                Box::new(Expression::TimeSeries(TimeSeries::new(
                    "bar".to_string(),
                    vec![],
                    None,
                    None,
                ))),
            ));

            assert_eq!(exp, expected_exp);
        });
    }

    #[test]
    fn parse_aggregation_operators() {
        let operators = vec![
            ("sum", SimpleAggregationOperator::Sum),
            ("avg", SimpleAggregationOperator::Avg),
        ];
        operators.iter().for_each(|(op_str, op)| {
            let query = format!("{op_str}(memory_consumption_bytes)");
            let lexer = Lexer::new(query);
            let mut parser = Parser::new(lexer).unwrap();

            let exp = parser.parse().unwrap();

            let expected_exp =
                Expression::AggregationExpression(Simple(SimpleAggregationExpression::new(
                    op.clone(),
                    Box::new(Expression::TimeSeries(TimeSeries::new(
                        "memory_consumption_bytes".to_string(),
                        vec![],
                        None,
                        None,
                    ))),
                    vec![],
                    false,
                )));
            assert_eq!(exp, expected_exp);

            let query = format!("{op_str} by (application) (memory_consumption_bytes)");
            let lexer = Lexer::new(query);
            let mut parser = Parser::new(lexer).unwrap();

            let exp = parser.parse().unwrap();

            let expected_exp =
                Expression::AggregationExpression(Simple(SimpleAggregationExpression::new(
                    op.clone(),
                    Box::new(Expression::TimeSeries(TimeSeries::new(
                        "memory_consumption_bytes".to_string(),
                        vec![],
                        None,
                        None,
                    ))),
                    vec!["application".to_string()],
                    false,
                )));
            assert_eq!(exp, expected_exp);
        });
    }

    #[test]
    fn parse_functions() {
        {
            let lexer = Lexer::new("time()".to_string());
            let mut parser = Parser::new(lexer).unwrap();

            let exp = parser.parse().unwrap();
            let expected_exp =
                Expression::CallExpression(CallExpression::new("time".to_string(), vec![]));

            assert_eq!(exp, expected_exp);
        }

        {
            let lexer = Lexer::new("vector(5)".to_string());
            let mut parser = Parser::new(lexer).unwrap();

            let exp = parser.parse().unwrap();
            let expected_exp = Expression::CallExpression(CallExpression::new(
                "vector".to_string(),
                vec![Box::new(Expression::FloatLiteral(FloatLiteral::new(
                    "5".to_string(),
                )))],
            ));

            assert_eq!(exp, expected_exp);
        }

        {
            let lexer = Lexer::new("clamp(foo, 1, 3)".to_string());
            let mut parser = Parser::new(lexer).unwrap();

            let exp = parser.parse().unwrap();
            let expected_exp = Expression::CallExpression(CallExpression::new(
                "clamp".to_string(),
                vec![
                    Box::new(Expression::TimeSeries(TimeSeries::new(
                        "foo".to_string(),
                        vec![],
                        None,
                        None,
                    ))),
                    Box::new(Expression::FloatLiteral(FloatLiteral::new("1".to_string()))),
                    Box::new(Expression::FloatLiteral(FloatLiteral::new("3".to_string()))),
                ],
            ));

            assert_eq!(exp, expected_exp);
        }
    }

    #[test]
    fn parse_histogram_quantile() {
        let lexer = Lexer::new(
            "histogram_quantile(0.9,sum(rate(prometheus_http_request_duration_seconds{ }[5m])))"
                .to_string(),
        );
        let mut parser = Parser::new(lexer).unwrap();

        let exp = parser.parse().unwrap();
        let expected_exp = Expression::CallExpression(CallExpression::new(
            "histogram_quantile".to_string(),
            vec![
                Box::new(Expression::FloatLiteral(FloatLiteral::new(
                    "0.9".to_string(),
                ))),
                Box::new(Expression::AggregationExpression(Simple(
                    SimpleAggregationExpression::new(
                        SimpleAggregationOperator::Sum,
                        Box::new(Expression::CallExpression(CallExpression::new(
                            "rate".to_string(),
                            vec![Box::new(Expression::TimeSeries(TimeSeries::new(
                                "prometheus_http_request_duration_seconds".to_string(),
                                vec![],
                                Some(TimeDuration {
                                    value: 5,
                                    unit: Minute,
                                }),
                                None,
                            )))],
                        ))),
                        vec![],
                        false,
                    ),
                ))),
            ],
        ));

        assert_eq!(exp, expected_exp);
    }
}
