use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext},
    head::Sample,
    query_exec::InstantSeries,
    query_exec::SeriesListInstantIterator,
};

pub fn eval_vector(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Scalar(num) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    let samples = context
        .query_points
        .into_iter()
        .map(|v| Sample::new(crate::head::TimestampSecond(v), num))
        .collect();

    let series = InstantSeries::new(vec![], samples);
    let iter = SeriesListInstantIterator::new(vec![series]);
    Ok(EvalValue::Instant(Box::new(iter)))
}

pub static VECTOR_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "vector",
    arg_types: &[ExpressionType::Scalar],
    return_type: ExpressionType::InstantVector,
    eval: eval_vector,
};
