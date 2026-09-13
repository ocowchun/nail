use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext},
    head::Sample,
    query_exec::{InstantSeries, InstantSeriesIterator, RangeSample, RangeSeriesIterator},
};

pub fn eval_rate(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Range(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    let iter = RateIterator::new(inner);
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

            // remove __name__label to follow prometheus rate function
            let labels = series
                .labels
                .into_iter()
                .filter(|l| l.name != "__name__")
                .collect();
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
        query_exec::{RangeSeries, SeriesListRangeIterator},
    };

    use super::*;

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
        let iter = SeriesListRangeIterator::new(vec![series1]);

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
}
