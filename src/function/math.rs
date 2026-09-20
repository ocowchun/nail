use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext, UnaryMapIterator},
};

pub fn eval_abs(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::abs,
    ))))
}

pub static ABS_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "abs",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_abs,
};

pub fn eval_ceil(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::ceil,
    ))))
}

pub static CEIL_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "ceil",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_ceil,
};

pub fn eval_floor(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::floor,
    ))))
}

pub static FLOOR_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "floor",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_floor,
};

pub fn eval_ln(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::ln,
    ))))
}

pub static LN_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "ln",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_ln,
};

pub fn eval_log2(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::log2,
    ))))
}

pub static LOG2_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "log2",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_log2,
};

pub fn eval_log10(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::log10,
    ))))
}

pub static LOG10_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "log10",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_log10,
};

pub fn eval_sqrt(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::sqrt,
    ))))
}

pub static SQRT_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "sqrt",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_sqrt,
};

pub fn eval_round(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator::new(
        inner,
        f64::round,
    ))))
}

pub static ROUND_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "round",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_round,
};

#[cfg(test)]
mod tests {
    use crate::{
        core::{Sample, TimestampSecond},
        query_exec::{InstantSeries, SeriesListInstantIterator},
    };

    use super::*;

    #[test]
    fn test_abs_function() {
        let series1 = {
            let labels = vec![];
            let samples = (0..5)
                .map(|i| {
                    let timestamp = TimestampSecond(i);
                    Sample::new(timestamp, i as f64)
                })
                .collect();
            InstantSeries::new(labels, samples)
        };
        let iter = SeriesListInstantIterator {
            series_list: vec![series1],
        };

        let context = QueryContext::new(vec![]);
        let res = eval_abs(vec![EvalValue::Instant(Box::new(iter))], context).unwrap();
        if let EvalValue::Instant(mut iter) = res {
            let mut actual = vec![];
            loop {
                match iter.next().unwrap() {
                    Some(sample) => {
                        actual.push(sample);
                    }
                    None => {
                        break;
                    }
                };
            }
            assert_eq!(actual.len(), 1);
            for (i, sample) in actual[0].samples.iter().enumerate() {
                assert_eq!(i as f64, sample.value);
            }
        } else {
            panic!("unexpected eval value");
        }
    }
}
