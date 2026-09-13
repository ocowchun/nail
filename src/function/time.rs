use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext},
    head::Sample,
    query_exec::InstantSeries,
    query_exec::SeriesListInstantIterator,
};

pub fn eval_time(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    if args.len() != 0 {
        unreachable!("validated by analyzer");
    }
    let samples = context
        .query_points
        .into_iter()
        .map(|v| Sample::new(crate::head::TimestampSecond(v), v as f64))
        .collect();

    let series = InstantSeries::new(vec![], samples);
    let iter = SeriesListInstantIterator::new(vec![series]);
    Ok(EvalValue::Instant(Box::new(iter)))
}

pub static TIME_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "time",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_time,
};
