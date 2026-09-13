use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext},
    head::{Sample, TimestampSecond},
    query_exec::{InstantSeries, InstantSeriesIterator},
};

struct AbsentIterator {
    inner: Box<dyn InstantSeriesIterator>,
    query_points: Vec<i64>,
    is_done: bool,
}

impl InstantSeriesIterator for AbsentIterator {
    fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
        if self.is_done {
            return Ok(None);
        }

        self.is_done = true;
        if let Some(_) = self.inner.next()? {
            return Ok(None);
        };

        let samples = self
            .query_points
            .iter()
            .map(|ts| Sample::new(TimestampSecond::new(ts.clone()), 1.0))
            .collect();
        let series = InstantSeries::new(vec![], samples);
        Ok(Some(series))
    }
}

pub fn eval_absent(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(AbsentIterator {
        inner,
        query_points: context.query_points.clone(),
        is_done: false,
    })))
}

pub static ABSENT_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "absent",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_absent,
};
