use crate::{
    analyzer::ExpressionType,
    ast::Expression,
    head::{Sample, TimestampSecond},
    query_exec::{InstantSeries, InstantSeriesIterator, RangeSample, RangeSeriesIterator},
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

pub fn eval_abs(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::abs,
    })))
}

pub static ABS_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "abs",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_abs,
};

pub fn eval_ceil(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::ceil,
    })))
}

pub static CEIL_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "ceil",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_ceil,
};

pub fn eval_floor(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::floor,
    })))
}

pub static FLOOR_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "floor",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_floor,
};

pub fn eval_ln(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::ln,
    })))
}

pub static LN_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "ln",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_ln,
};

pub fn eval_log2(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::log2,
    })))
}

pub static LOG2_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "log2",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_log2,
};

pub fn eval_log10(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::log10,
    })))
}

pub static LOG10_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "log10",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_log10,
};

pub fn eval_sqrt(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::sqrt,
    })))
}

pub static SQRT_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "sqrt",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_sqrt,
};

pub fn eval_round(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform: f64::round,
    })))
}

pub static ROUND_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "round",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_round,
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
        return Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
            inner,
            transform: |_| f64::NAN,
        })));
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
    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform,
    })))
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
        return Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
            inner,
            transform: |_| f64::NAN,
        })));
    }

    let transform = move |f| {
        if f > max { max } else { f }
    };
    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform,
    })))
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
        return Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
            inner,
            transform: |_| f64::NAN,
        })));
    }

    let transform = move |f| {
        if f < min { min } else { f }
    };
    Ok(EvalValue::Instant(Box::new(UnaryMapIterator {
        inner,
        transform,
    })))
}

pub static CLAMP_MIN_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "clamp_min",
    arg_types: &[ExpressionType::InstantVector, ExpressionType::Scalar],
    return_type: ExpressionType::InstantVector,
    eval: eval_clamp_min,
};

struct UnaryMapIterator<F> {
    inner: Box<dyn InstantSeriesIterator>,
    transform: F,
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

struct SeriesListInstantIterator {
    series_list: Vec<InstantSeries>,
}

impl SeriesListInstantIterator {
    pub fn new(series_list: Vec<InstantSeries>) -> Self {
        Self { series_list }
    }
}

impl InstantSeriesIterator for SeriesListInstantIterator {
    fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
        if let Some(series) = self.series_list.pop() {
            return Ok(Some(series));
        }
        return Ok(None);
    }
}

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

pub fn eval_rate(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Range(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    let iter = RateIterator { inner };
    Ok(EvalValue::Instant(Box::new(iter)))
}

struct RateIterator {
    inner: Box<dyn RangeSeriesIterator>,
}

impl RateIterator {
    pub fn new(inner: Box<dyn RangeSeriesIterator>) -> Self {
        Self { inner }
    }

    fn rate(range_sample: RangeSample) -> Option<f64> {
        let samples = range_sample.samples;
        let range_start_secs = range_sample.range.start.0 as f64;
        let range_end_secs = range_sample.range.end.0 as f64;
        let first = samples.first()?;
        let last = samples.last()?;
        if samples.len() < 2 {
            return None;
        }

        let mut increase = last.value - first.value;

        for pair in samples.windows(2) {
            if pair[1].value < pair[0].value {
                increase += pair[0].value; // counter reset
            }
        }

        let sampled_interval = (last.timestamp.0 - first.timestamp.0) as f64;
        let average_interval = sampled_interval as f64 / (samples.len() - 1) as f64;

        let mut to_start = (first.timestamp.0 as f64 - range_start_secs) as f64;
        let mut to_end = (range_end_secs - last.timestamp.0 as f64) as f64;
        let threshold = average_interval * 1.1;

        if to_start >= threshold {
            to_start = average_interval / 2.0;
        }

        // Do not extrapolate a counter into negative values.
        if increase > 0.0 && first.value >= 0.0 {
            let time_to_zero = sampled_interval * first.value / increase;
            to_start = to_start.min(time_to_zero);
        }

        if to_end >= threshold {
            to_end = average_interval / 2.0;
        }

        let extrapolated_increase =
            increase * (sampled_interval + to_start + to_end) / sampled_interval;

        Some(extrapolated_increase / (range_end_secs - range_start_secs))
    }
}

impl InstantSeriesIterator for RateIterator {
    fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
        let series = self.inner.as_mut().next()?;

        let mut samples = vec![];
        if let Some(series) = series {
            for sample in series.samples.into_iter() {
                let timestamp = sample.range.end.clone();
                if let Some(val) = RateIterator::rate(sample) {
                    samples.push(Sample::new(timestamp, val));
                }
            }

            let labels = series.labels;
            Ok(Some(InstantSeries::new(labels, samples)))
        } else {
            Ok(None)
        }
    }
}

pub static RATE_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "rate",
    arg_types: &[ExpressionType::RangeVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_rate,
};

#[cfg(test)]
mod tests {
    use crate::{
        head::{Sample, TimeRange, TimestampSecond},
        query_exec::{InstantSeries, RangeSeries},
    };

    use super::*;

    struct DummyInstantIterator {
        series_list: Vec<InstantSeries>,
    }

    impl InstantSeriesIterator for DummyInstantIterator {
        fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
            if let Some(series) = self.series_list.pop() {
                return Ok(Some(series));
            }
            return Ok(None);
        }
    }

    struct DummyRangeIterator {
        series_list: Vec<RangeSeries>,
    }

    impl RangeSeriesIterator for DummyRangeIterator {
        fn next(&mut self) -> Result<Option<RangeSeries>, String> {
            if let Some(series) = self.series_list.pop() {
                return Ok(Some(series));
            }
            return Ok(None);
        }
    }

    #[test]
    fn test_rate_function() {
        let series1 = {
            let labels = vec![];
            let samples = (0..5)
                .map(|i| {
                    let start = TimestampSecond(i);
                    let end = TimestampSecond(i + 3);
                    let samples = (start.0..(end.0))
                        .map(|j| Sample::new(TimestampSecond(j), j as f64))
                        .collect();
                    RangeSample::new(TimeRange::new(start, end), samples)
                })
                .collect();
            RangeSeries::new(labels, samples)
        };
        let iter = DummyRangeIterator {
            series_list: vec![series1],
        };

        let context = QueryContext::new(vec![]);
        let res = eval_rate(vec![EvalValue::Range(Box::new(iter))], context).unwrap();
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
            for (_, sample) in actual[0].samples.iter().enumerate() {
                assert_eq!(1.0, sample.value);
            }
        } else {
            panic!("unexpected eval value");
        }
    }

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
        let iter = DummyInstantIterator {
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
