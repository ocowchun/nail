use std::collections::HashSet;

use crate::{
    analyzer::ExpressionType,
    function::types::{EvalValue, FunctionSpec, QueryContext},
    head::{Sample, TimestampSecond},
    query_exec::{InstantSeries, InstantSeriesIterator, RangeSeriesIterator},
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

        let mut set = HashSet::new();
        loop {
            if let Some(series) = self.inner.next()? {
                for sample in series.samples.into_iter() {
                    set.insert(sample.timestamp.0);
                }
            } else {
                break;
            }
        }

        let mut samples = vec![];
        for ts in self.query_points.iter() {
            if !set.contains(ts) {
                samples.push(Sample::new(TimestampSecond::new(ts.clone()), 1.0));
            }
        }

        let series = InstantSeries::new(vec![], samples);
        Ok(Some(series))
    }
}

fn eval_absent(mut args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
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

struct AbsentOverTimeIterator {
    inner: Box<dyn RangeSeriesIterator>,
    query_points: Vec<i64>,
    is_done: bool,
}

impl InstantSeriesIterator for AbsentOverTimeIterator {
    fn next(&mut self) -> Result<Option<crate::query_exec::InstantSeries>, String> {
        if self.is_done {
            return Ok(None);
        }

        self.is_done = true;

        let mut set = HashSet::new();
        loop {
            if let Some(series) = self.inner.next()? {
                for sample in series.samples.into_iter() {
                    set.insert(sample.range.end.0);
                }
            } else {
                break;
            }
        }

        let mut samples = vec![];
        for ts in self.query_points.iter() {
            if !set.contains(ts) {
                samples.push(Sample::new(TimestampSecond::new(ts.clone()), 1.0));
            }
        }

        let series = InstantSeries::new(vec![], samples);
        Ok(Some(series))
    }
}

fn eval_absent_over_time(
    mut args: Vec<EvalValue>,
    context: QueryContext,
) -> Result<EvalValue, String> {
    let EvalValue::Range(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    Ok(EvalValue::Instant(Box::new(AbsentOverTimeIterator {
        inner,
        query_points: context.query_points.clone(),
        is_done: false,
    })))
}

pub static ABSENT_OVER_TIME_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "absent_over_time",
    arg_types: &[ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_absent_over_time,
};

#[cfg(test)]
mod tests {
    use crate::{
        head::{Sample, TimeRange, TimestampSecond},
        query_exec::{
            InstantSeries, InstantSeriesIterator, RangeSample, RangeSeries, SeriesListRangeIterator,
        },
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

    #[test]
    fn test_absent_function() {
        let series1 = {
            let labels = vec![];
            let samples = (0..2)
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

        let context = QueryContext::new(vec![0, 1, 2, 3]);
        let res = eval_absent(vec![EvalValue::Instant(Box::new(iter))], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected = vec![
                Sample::new(TimestampSecond(2), 1.0),
                Sample::new(TimestampSecond(3), 1.0),
            ];
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_absent_over_time_function() {
        let series1 = {
            let labels = vec![];
            let samples = (0..2)
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

        let context = QueryContext::new(vec![3, 4, 5, 6]);
        let res = eval_absent_over_time(vec![EvalValue::Range(Box::new(iter))], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected = vec![
                Sample::new(TimestampSecond(5), 1.0),
                Sample::new(TimestampSecond(6), 1.0),
            ];
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }
}
