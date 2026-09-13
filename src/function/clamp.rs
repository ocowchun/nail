use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext, UnaryMapIterator},
    query_exec::SeriesListInstantIterator,
};

pub fn eval_clamp(mut args: Vec<EvalValue>, _: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };
    let EvalValue::Scalar(min) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };
    let EvalValue::Scalar(max) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };
    if min > max {
        let iter = SeriesListInstantIterator::new(vec![]);
        return Ok(EvalValue::Instant(Box::new(iter)));
    }

    if min.is_nan() || max.is_nan() {
        return Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
            inner,
            |_| f64::NAN,
        ))));
    }

    let transform = move |f| {
        if f < min {
            min
        } else if f > max {
            max
        } else {
            f
        }
    };
    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner, transform,
    ))))
}

pub static CLAMP_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "clamp",
    arg_types: &[
        ExpressionType::InstantVector,
        ExpressionType::Scalar,
        ExpressionType::Scalar,
    ],
    return_type: ExpressionType::InstantVector,
    eval: eval_clamp,
};

pub fn eval_clamp_max(mut args: Vec<EvalValue>, _: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };
    let EvalValue::Scalar(max) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    if max.is_nan() {
        return Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
            inner,
            |_| f64::NAN,
        ))));
    }

    let transform = move |f| {
        if f > max { max } else { f }
    };
    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner, transform,
    ))))
}

pub static CLAMP_MAX_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "clamp_max",
    arg_types: &[ExpressionType::InstantVector, ExpressionType::Scalar],
    return_type: ExpressionType::InstantVector,
    eval: eval_clamp_max,
};

pub fn eval_clamp_min(mut args: Vec<EvalValue>, _: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };
    let EvalValue::Scalar(min) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    if min.is_nan() {
        return Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
            inner,
            |_| f64::NAN,
        ))));
    }

    let transform = move |f| {
        if f < min { min } else { f }
    };
    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner, transform,
    ))))
}

pub static CLAMP_MIN_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "clamp_min",
    arg_types: &[ExpressionType::InstantVector, ExpressionType::Scalar],
    return_type: ExpressionType::InstantVector,
    eval: eval_clamp_min,
};
