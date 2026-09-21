use crate::{
    analyzer::ExpressionType,
    core::{Sample, TimestampSecond},
    function::{EvalValue, FunctionSpec, QueryContext},
    query_exec::{InstantSeries, InstantSeriesIterator, RangeSample, RangeSeriesIterator},
};

struct LinearRegressionResult {
    slope: f64,
    intercept: f64,
}

fn linear_regression(range_sample: RangeSample) -> LinearRegressionResult {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    let intercept_time = range_sample.range.end;

    let mut const_y = true;
    let first_y = range_sample.samples.first().unwrap().value;
    let n = range_sample.samples.len() as f64;

    for (i, sample) in range_sample.samples.into_iter().enumerate() {
        if const_y && i > 0 && sample.value != first_y {
            const_y = false;
        }
        let x = (sample.timestamp.0 - intercept_time.0) as f64;
        // TODO: consider kahansum
        // https://github.com/prometheus/prometheus/blob/main/util/kahansum/kahansum.go#L25
        sum_x += x;
        sum_y += sample.value;
        sum_xy += x * sample.value;
        sum_x2 += x * x;
    }

    if const_y {
        if first_y == f64::INFINITY {
            return LinearRegressionResult {
                slope: f64::NAN,
                intercept: f64::NAN,
            };
        }
        return LinearRegressionResult {
            slope: 0.0,
            intercept: first_y,
        };
    }

    let cov_xy = sum_xy - sum_x * sum_y / n;
    let var_x = sum_x2 - sum_x * sum_x / n;

    let slope = cov_xy / var_x;
    let intercept = sum_y / n - slope * sum_x / n;
    LinearRegressionResult { slope, intercept }
}

struct DerivIterator {
    inner: Box<dyn RangeSeriesIterator>,
}

impl InstantSeriesIterator for DerivIterator {
    fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
        let series = self.inner.next()?;

        if let Some(series) = series {
            let samples: Vec<_> = series
                .samples
                .into_iter()
                .map(|range_sample| {
                    let timestamp = range_sample.range.end.clone();
                    let res = linear_regression(range_sample);
                    Sample::new(timestamp, res.slope)
                })
                .collect();

            // remove __name__label to follow prometheus convention
            let labels = series
                .labels
                .into_iter()
                .filter(|l| l.name != "__name__")
                .collect();
            Ok(Some(InstantSeries::new(labels, samples)))
        } else {
            return Ok(None);
        }
    }
}

fn eval_deriv(mut args: Vec<EvalValue>, _context: QueryContext) -> Result<EvalValue, String> {
    let EvalValue::Range(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    let iter = DerivIterator { inner };
    Ok(EvalValue::Instant(Box::new(iter)))
}

pub static DERIV_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "deriv",
    arg_types: &[ExpressionType::RangeVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_deriv,
};

#[cfg(test)]
mod tests {
    use crate::{
        core::{Label, Sample, TimestampSecond},
        head::TimeRange,
        query_exec::{RangeSeries, SeriesListRangeIterator},
    };

    use super::*;

    #[test]
    fn test_linear_regression() {
        let samples = vec![
            Sample::new(TimestampSecond(1), 2.0),
            Sample::new(TimestampSecond(2), 4.0),
            Sample::new(TimestampSecond(3), 6.0),
        ];
        let range_sample = RangeSample::new(
            TimeRange::new(TimestampSecond(0), TimestampSecond(3)),
            samples,
        );
        let res = linear_regression(range_sample);

        assert_eq!(res.slope, 2.0);
        assert_eq!(res.intercept, 6.0);
    }

    #[test]
    fn test_deriv_function() {
        let series_group = vec!["foo", "bar"]
            .iter()
            .map(|service| {
                let labels = vec![
                    Label::new("__name__".to_owned(), "my_counter".to_owned()),
                    Label::new("service".to_owned(), service.to_string()),
                ];
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
            })
            .collect();
        let iter = SeriesListRangeIterator::new(series_group);

        let context = QueryContext::new(vec![]);
        let res = eval_deriv(vec![EvalValue::Range(Box::new(iter))], context).unwrap();
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
            assert_eq!(actual.len(), 2);
            for (_, sample) in actual[0].samples.iter().enumerate() {
                assert_eq!(sample.value, 1.0);
            }
        } else {
            panic!("unexpected eval value");
        }
    }
}
