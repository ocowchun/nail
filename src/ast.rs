use std::{
    fmt::{self, format},
    time::Duration,
};

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum Expression {
    FloatLiteral(FloatLiteral),
    StringLiteral(String),
    TimeSeries(TimeSeries),
    BinaryExpression(BinaryExpression),
    CallExpression(CallExpression),
    AggregationExpression(AggregationExpression),
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct FloatLiteral {
    literal: String,
}

impl FloatLiteral {
    pub fn new(literal: String) -> FloatLiteral {
        FloatLiteral { literal }
    }
    pub fn value(&self) -> f64 {
        self.literal.parse::<f64>().unwrap()
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum TimeUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Year,
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct TimeDuration {
    pub value: i64,
    pub unit: TimeUnit,
}

impl TimeDuration {
    pub fn abs(&self) -> Result<TimeDuration, String> {
        let (value, is_overflow) = self.value.overflowing_abs();
        if is_overflow {
            return Err(format!(
                "failed to build abs TimeDuration because the abs({}) is overflow",
                self.value,
            ));
        }

        Ok(TimeDuration {
            value,
            unit: self.unit.clone(),
        })
    }

    pub fn from(literal: String) -> Result<TimeDuration, String> {
        let pos = literal
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(literal.len());
        let (value, unit) = literal.split_at(pos);

        let value = match value.parse::<i64>() {
            Ok(v) => v,
            Err(err) => {
                return Err(err.to_string());
            }
        };
        let unit = match unit {
            "ms" => TimeUnit::Millisecond,
            "s" => TimeUnit::Second,
            "m" => TimeUnit::Minute,
            "h" => TimeUnit::Hour,
            "d" => TimeUnit::Day,
            "w" => TimeUnit::Week,
            "y" => TimeUnit::Year,
            _ => {
                return Err(format!(
                    "unexpected char `{unit}` when trying to parse timeduration"
                ));
            }
        };

        Ok(TimeDuration { value, unit })
    }
}

impl TryFrom<&TimeDuration> for Duration {
    type Error = String;
    fn try_from(duration: &TimeDuration) -> Result<Self, Self::Error> {
        let value =
            u64::try_from(duration.value).map_err(|_| "duration cannot be negative".to_string())?;

        match duration.unit {
            TimeUnit::Millisecond => Ok(Duration::from_millis(value)),
            TimeUnit::Second => Ok(Duration::from_secs(value)),
            TimeUnit::Minute => checked_secs(value, 60),
            TimeUnit::Hour => checked_secs(value, 60 * 60),
            TimeUnit::Day => checked_secs(value, 24 * 60 * 60),
            TimeUnit::Week => checked_secs(value, 7 * 24 * 60 * 60),
            TimeUnit::Year => checked_secs(value, 365 * 24 * 60 * 60),
        }
    }
}

fn checked_secs(value: u64, multiplier: u64) -> Result<Duration, String> {
    value
        .checked_mul(multiplier)
        .map(Duration::from_secs)
        .ok_or_else(|| "duration is too large".to_string())
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct TimeSeries {
    pub name: String,
    pub label_matchers: Vec<LabelMatcher>,
    pub range: Option<TimeDuration>,
    pub offset: Option<TimeDuration>,
}

impl TimeSeries {
    pub fn new(
        name: String,
        label_matchers: Vec<LabelMatcher>,
        range: Option<TimeDuration>,
        offset: Option<TimeDuration>,
    ) -> Self {
        Self {
            name,
            label_matchers,
            range,
            offset,
        }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum LabelMatcherOperator {
    Equal,
    NotEqual,
    RegexMatch,
    NotRegexMatch,
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct LabelMatcher {
    pub operator: LabelMatcherOperator,
    pub label_name: String,
    pub label_value: String,
}

impl LabelMatcher {
    pub fn new(operator: LabelMatcherOperator, label_name: String, label_value: String) -> Self {
        Self {
            operator,
            label_name,
            label_value,
        }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum BinaryOperator {
    Plus,
    Minus,
    Multiply,
    Divide,
    Mod,
    Power,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
    NotEqual,
    And,
    Unless,
    Or,
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            BinaryOperator::Plus => "+",
            BinaryOperator::Minus => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::Mod => "%",
            BinaryOperator::Power => "^",
            BinaryOperator::LessThan => "<",
            BinaryOperator::LessThanOrEqual => "<=",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::GreaterThanOrEqual => ">=",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::And => "and",
            BinaryOperator::Unless => "unless",
            BinaryOperator::Or => "or",
        };
        f.write_str(symbol)
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct BinaryExpression {
    pub operator: BinaryOperator,
    pub lhs: Box<Expression>,
    pub rhs: Box<Expression>,
}

impl BinaryExpression {
    pub fn new(operator: BinaryOperator, lhs: Box<Expression>, rhs: Box<Expression>) -> Self {
        Self { operator, lhs, rhs }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct CallExpression {
    pub name: String,
    pub arguments: Vec<Box<Expression>>,
}

impl CallExpression {
    pub fn new(name: String, arguments: Vec<Box<Expression>>) -> Self {
        Self { name, arguments }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum SimpleAggregationOperator {
    Sum,
    Avg,
    Min,
    Max,
    Group,
    Count,
    Stddev,
    Stdvar,
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum AggregationExpression {
    Simple(SimpleAggregationExpression),
    K(KAggregationExpression),
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct SimpleAggregationExpression {
    pub op: SimpleAggregationOperator,
    pub exp: Box<Expression>,
    pub labels: Vec<String>,
    pub is_without: bool,
}

impl SimpleAggregationExpression {
    pub fn new(
        op: SimpleAggregationOperator,
        exp: Box<Expression>,
        labels: Vec<String>,
        is_without: bool,
    ) -> Self {
        Self {
            op,
            exp,
            labels,
            is_without,
        }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum KAggregationOperator {
    BottomK,
    TopK,
    LimitK,
    LimitRatio,
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct KAggregationExpression {
    pub op: KAggregationOperator,
    pub exp: Box<Expression>,
    pub labels: Vec<String>,
    pub is_without: bool,
}

impl KAggregationExpression {
    pub fn new(
        op: KAggregationOperator,
        exp: Box<Expression>,
        labels: Vec<String>,
        is_without: bool,
    ) -> Self {
        Self {
            op,
            exp,
            labels,
            is_without,
        }
    }
}

// TODO
// LimitRatio,
// CountValues,
// Quantile,
