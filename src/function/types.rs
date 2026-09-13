use crate::{
    analyzer::ExpressionType,
    query_exec::{InstantSeriesIterator, RangeSeriesIterator},
};

pub enum EvalValue {
    Instant(Box<dyn InstantSeriesIterator>),
    Range(Box<dyn RangeSeriesIterator>),
    Scalar(f64),
    String(String),
}

pub struct QueryContext {
    pub query_points: Vec<i64>,
}

impl QueryContext {
    pub fn new(query_points: Vec<i64>) -> Self {
        Self { query_points }
    }
}

pub type EvalFunction =
    fn(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String>;

#[derive(Debug, Clone)]
pub struct FunctionSpec {
    pub name: &'static str,
    pub arg_types: &'static [ExpressionType],
    pub return_type: ExpressionType,
    pub eval: EvalFunction,
}

pub struct UnaryMapIterator<F> {
    inner: Box<dyn InstantSeriesIterator>,
    transform: F,
}

impl<F> UnaryMapIterator<F>
where
    F: Fn(f64) -> f64,
{
    pub fn new(inner: Box<dyn InstantSeriesIterator>, transform: F) -> Self {
        Self { inner, transform }
    }
}

impl<F> InstantSeriesIterator for UnaryMapIterator<F>
where
    F: Fn(f64) -> f64,
{
    fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
        let Some(mut series) = self.inner.next()? else {
            return Ok(None);
        };

        for sample in &mut series.samples {
            sample.value = (self.transform)(sample.value);
        }
        Ok(Some(series))
    }
}
