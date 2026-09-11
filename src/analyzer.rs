use std::{
    fmt::{self, format},
    time::Duration,
};

use crate::{
    ast::{
        AggregationExpression::{self, K, Simple},
        BinaryExpression, BinaryOperator, CallExpression, Expression, LabelMatcher,
        LabelMatcherOperator, SimpleAggregationExpression, SimpleAggregationOperator, TimeSeries,
    },
    function::{ABS_FUNCTION_SPEC, EvalValue, FunctionSpec, RATE_FUNCTION_SPEC},
};

#[derive(PartialEq, Debug, Clone, Eq)]
pub enum ExpressionType {
    InstantVector,
    RangeVector,
    Scalar,
    String,
}

impl fmt::Display for ExpressionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            ExpressionType::InstantVector => "InstantVector",
            ExpressionType::RangeVector => "RangeVector",
            ExpressionType::Scalar => "Scalar",
            ExpressionType::String => "String",
        };
        f.write_str(symbol)
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum Plan {
    Number(f64),
    String(String),
    InstantSelector(InstantSelector),
    RangeSelector(RangeSelector),
    Call(Call),
    Binary(Binary),
    SimpleAgg(SimpleAgg),
}

impl Plan {
    pub fn return_type(&self) -> ExpressionType {
        match self {
            Plan::Number(_) => ExpressionType::Scalar,
            Plan::String(_) => ExpressionType::String,
            Plan::InstantSelector(_) => ExpressionType::InstantVector,
            Plan::RangeSelector(_) => ExpressionType::RangeVector,
            Plan::Call(call) => call.spec.return_type.clone(),
            Plan::Binary(binary) => binary.return_type.clone(),
            Plan::SimpleAgg(_) => ExpressionType::InstantVector,
        }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct Offset {
    duration: Duration,
    is_negative: bool,
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct InstantSelector {
    pub matchers: Vec<LabelMatcher>,
    pub offset: Offset,
}

impl InstantSelector {
    pub fn new(matchers: Vec<LabelMatcher>, offset: Offset) -> Self {
        Self { matchers, offset }
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct RangeSelector {
    pub matchers: Vec<LabelMatcher>,
    pub range: Duration,
    pub offset: Offset,
}

impl RangeSelector {
    pub fn new(matchers: Vec<LabelMatcher>, range: Duration, offset: Offset) -> Self {
        Self {
            matchers,
            range,
            offset,
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Call {
    pub spec: &'static FunctionSpec,
    pub args: Vec<Plan>,
    // pub return_type: ExpressionType,
}

impl Call {
    pub fn new(spec: &'static FunctionSpec, args: Vec<Plan>) -> Self {
        Self { spec, args }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Binary {
    pub op: BinaryOperator,
    pub lhs: Box<Plan>,
    pub rhs: Box<Plan>,
    pub return_type: ExpressionType,
}

impl Binary {
    pub fn new(
        op: BinaryOperator,
        lhs: Box<Plan>,
        rhs: Box<Plan>,
        return_type: ExpressionType,
    ) -> Self {
        Self {
            op,
            lhs,
            rhs,
            return_type,
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct SimpleAgg {
    pub op: SimpleAggregationOperator,
    pub inner_plan: Box<Plan>,
    pub is_without: bool,
    pub labels: Vec<String>,
}

impl SimpleAgg {
    pub fn new(
        op: SimpleAggregationOperator,
        inner_plan: Box<Plan>,
        is_without: bool,
        labels: Vec<String>,
    ) -> Self {
        Self {
            op,
            inner_plan,
            is_without,
            labels,
        }
    }
}

pub struct Analyzer {}

impl Analyzer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn analyze(&self, expression: &Expression) -> Result<Plan, String> {
        match expression {
            Expression::FloatLiteral(f) => Ok(Plan::Number(f.value())),
            Expression::StringLiteral(s) => Ok(Plan::String(s.to_string())),
            Expression::TimeSeries(ts) => self.analyze_timeseries(ts),
            Expression::BinaryExpression(binary_exp) => self.analyze_binary_expression(binary_exp),
            Expression::CallExpression(call_exp) => self.analyze_call_expression(call_exp),
            Expression::AggregationExpression(agg_exp) => {
                self.analyze_aggregation_expression(agg_exp)
            }
        }
    }

    fn analyze_timeseries(&self, timeseries: &TimeSeries) -> Result<Plan, String> {
        let offset = match timeseries.offset.as_ref() {
            Some(offset) => Offset {
                is_negative: offset.value.is_negative(),
                duration: Duration::try_from(&offset.abs()?)?,
            },
            None => Offset {
                duration: Duration::new(0, 0),
                is_negative: false,
            },
        };

        let mut label_matchers = timeseries.label_matchers.clone();
        label_matchers.push(LabelMatcher::new(
            LabelMatcherOperator::Equal,
            "__name__".to_string(),
            timeseries.name.to_string(),
        ));

        match timeseries.range.as_ref() {
            Some(range) => {
                let range = Duration::try_from(range)?;
                let selector = RangeSelector::new(label_matchers, range, offset);
                Ok(Plan::RangeSelector(selector))
            }
            None => {
                let selector = InstantSelector::new(label_matchers, offset);
                Ok(Plan::InstantSelector(selector))
            }
        }
    }

    fn analyze_binary_expression(&self, expression: &BinaryExpression) -> Result<Plan, String> {
        let left_plan = self.analyze(expression.lhs.as_ref())?;
        let right_plan = self.analyze(expression.rhs.as_ref())?;

        match (left_plan.return_type(), right_plan.return_type()) {
            (ExpressionType::Scalar, ExpressionType::Scalar) => {
                let plan = Binary::new(
                    expression.operator.clone(),
                    Box::new(left_plan),
                    Box::new(right_plan),
                    ExpressionType::Scalar,
                );
                return Ok(Plan::Binary(plan));
            }
            (ExpressionType::InstantVector, ExpressionType::Scalar) => {
                let plan = Binary::new(
                    expression.operator.clone(),
                    Box::new(left_plan),
                    Box::new(right_plan),
                    ExpressionType::InstantVector,
                );
                return Ok(Plan::Binary(plan));
            }
            (ExpressionType::Scalar, ExpressionType::InstantVector) => {
                let plan = Binary::new(
                    expression.operator.clone(),
                    Box::new(left_plan),
                    Box::new(right_plan),
                    ExpressionType::InstantVector,
                );
                return Ok(Plan::Binary(plan));
            }
            (ExpressionType::InstantVector, ExpressionType::InstantVector) => {
                let plan = Binary::new(
                    expression.operator.clone(),
                    Box::new(left_plan),
                    Box::new(right_plan),
                    ExpressionType::InstantVector,
                );
                return Ok(Plan::Binary(plan));
            }
            _ => {
                return Err(format!(
                    "binary operator {} does not support range vectors",
                    expression.operator
                ));
            }
        }
    }

    fn analyze_call_expression(&self, expression: &CallExpression) -> Result<Plan, String> {
        let mut args = vec![];
        for arg in expression.arguments.iter() {
            let arg = self.analyze(arg)?;
            args.push(arg);
        }

        let spec = match expression.name.as_ref() {
            "abs" => &ABS_FUNCTION_SPEC,
            "rate" => &RATE_FUNCTION_SPEC,
            _ => {
                return Err(format!("unsupported function {}", expression.name));
            }
        };
        if args.len() != spec.arg_types.len() {
            return Err(format!(
                "{} required {} argument",
                spec.name,
                spec.arg_types.len()
            ));
        }

        for (i, arg) in args.iter().enumerate() {
            if arg.return_type() != spec.arg_types[i] {
                return Err(format!(
                    "{} args[{}] should be {}",
                    spec.name, i, spec.arg_types[i]
                ));
            }
        }

        // TODO: refactor it later to support multiple functions
        // if expression.name == "abs" {
        //     if expression.arguments.len() != 1 {
        //         return Err(format!("abs required 1 argument"));
        //     }

        //     let exp = expression.arguments.first().unwrap();
        //     let arg = self.analyze(exp)?;
        //     if arg.return_type() != ExpressionType::InstantVector {
        //         return Err(format!("argument must be instant-vector"));
        //     }

        //     let plan = Call::new(
        //         expression.name.to_string(),
        //         vec![arg],
        //         ExpressionType::InstantVector,
        //     );
        //     return Ok(Plan::Call(plan));
        // } else {
        //     return Err(format!("unsupported function {}", expression.name));
        // }
        todo!()
    }

    fn analyze_aggregation_expression(
        &self,
        expression: &AggregationExpression,
    ) -> Result<Plan, String> {
        match expression {
            AggregationExpression::Simple(exp) => self.analyze_simple_aggregation_expression(exp),
            AggregationExpression::K(_) => {
                return Err(format!("wip"));
            }
        }
    }

    fn analyze_simple_aggregation_expression(
        &self,
        expression: &SimpleAggregationExpression,
    ) -> Result<Plan, String> {
        let inner_plan = self.analyze(&expression.exp)?;
        if inner_plan.return_type() != ExpressionType::InstantVector {
            return Err(format!(
                "expected aggregation inner expreesion to be InstantVector got {}",
                inner_plan.return_type()
            ));
        }
        let plan = SimpleAgg::new(
            expression.op.clone(),
            Box::new(inner_plan),
            expression.is_without,
            expression.labels.clone(),
        );
        Ok(Plan::SimpleAgg(plan))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        analyzer::{
            Analyzer, Binary, BinaryOperator, ExpressionType, InstantSelector, Offset, Plan,
            RangeSelector, SimpleAgg,
        },
        ast::{LabelMatcher, LabelMatcherOperator},
        lexer::Lexer,
        parser::Parser,
    };

    #[test]
    fn analyze_plan() {
        let lexer = Lexer::new("http_requests_total".to_string());
        let mut parser = Parser::new(lexer);
        let exp = parser.parse().unwrap();

        let analyzer = Analyzer {};

        let plan = analyzer.analyze(&exp).unwrap();

        let offset = Offset {
            duration: Duration::new(0, 0),
            is_negative: false,
        };
        let expected_plan = Plan::InstantSelector(InstantSelector::new(
            vec![LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "http_requests_total".to_string(),
            )],
            offset,
        ));
        assert_eq!(plan, expected_plan);
    }

    #[test]
    fn analyze_plan2() {
        let lexer = Lexer::new("http_requests_total{job=\"prometheus\"}[5m]".to_string());
        let mut parser = Parser::new(lexer);
        let exp = parser.parse().unwrap();

        let analyzer = Analyzer {};

        let plan = analyzer.analyze(&exp).unwrap();

        let matchers = vec![
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "job".to_string(),
                "prometheus".to_string(),
            ),
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "http_requests_total".to_string(),
            ),
        ];
        let range = Duration::new(300, 0);
        let offset = Offset {
            duration: Duration::new(0, 0),
            is_negative: false,
        };
        let expected_plan = Plan::RangeSelector(RangeSelector::new(matchers, range, offset));
        assert_eq!(plan, expected_plan);
    }

    #[test]
    fn analyze_plan3() {
        let lexer = Lexer::new("http_requests_total{job=\"prometheus\"} / 100".to_string());
        let mut parser = Parser::new(lexer);
        let exp = parser.parse().unwrap();

        let analyzer = Analyzer {};

        let plan = analyzer.analyze(&exp).unwrap();

        let matchers = vec![
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "job".to_string(),
                "prometheus".to_string(),
            ),
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "http_requests_total".to_string(),
            ),
        ];
        let offset = Offset {
            duration: Duration::new(0, 0),
            is_negative: false,
        };
        let lhs = Plan::InstantSelector(InstantSelector::new(matchers, offset));
        let rhs = Plan::Number(100.0);
        let expected_plan = Plan::Binary(Binary::new(
            BinaryOperator::Divide,
            Box::new(lhs),
            Box::new(rhs),
            ExpressionType::InstantVector,
        ));

        assert_eq!(plan, expected_plan);
    }

    #[test]
    fn analyze_plan4() {
        let lexer =
            Lexer::new("sum by (application) (memory_consumption_bytes{job=\"nail\"})".to_string());
        let mut parser = Parser::new(lexer);
        let exp = parser.parse().unwrap();

        let analyzer = Analyzer {};

        let plan = analyzer.analyze(&exp).unwrap();

        let matchers = vec![
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "job".to_string(),
                "nail".to_string(),
            ),
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "memory_consumption_bytes".to_string(),
            ),
        ];
        let offset = Offset {
            duration: Duration::new(0, 0),
            is_negative: false,
        };
        let inner_plan = Plan::InstantSelector(InstantSelector::new(matchers, offset));
        let expected_plan = Plan::SimpleAgg(SimpleAgg::new(
            crate::ast::SimpleAggregationOperator::Sum,
            Box::new(inner_plan),
            false,
            vec!["application".to_string()],
        ));

        assert_eq!(plan, expected_plan);
    }
}
