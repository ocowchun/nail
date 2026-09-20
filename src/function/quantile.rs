use std::{
    collections::{HashMap, HashSet},
    fmt::format,
};

use crate::{
    analyzer::ExpressionType,
    core::{Sample, SeriesKey, TimestampSecond},
    function::types::{EvalValue, FunctionSpec, QueryContext},
    query_exec::{InstantSeries, InstantSeriesIterator},
};

struct Bucket {
    upper_bound: f64,
    cumulative_count: f64,
}

/// Calculates `phi` for one classic histogram.
///
/// `buckets` must be sorted by increasing upper bound, their cumulative counts
/// must be non-negative and monotonically non-decreasing, and the final bucket
/// must be `+Inf`. `phi` must be finite and within `[0.0, 1.0]`.
fn histogram_quantile(phi: f64, buckets: &[Bucket]) -> f64 {
    if buckets.len() < 2 {
        return f64::NAN;
    } else if buckets.last().unwrap().upper_bound != f64::INFINITY {
        return f64::NAN;
    }

    let mut rank = buckets.last().unwrap().cumulative_count * phi;
    let idx = buckets.partition_point(|b| b.cumulative_count < rank);

    let val = if idx == buckets.len() - 1 {
        buckets[idx - 1].upper_bound
    } else {
        let mut start = 0.0;
        let end = buckets[idx].upper_bound;
        let mut count = buckets[idx].cumulative_count;
        if idx > 0 {
            start = buckets[idx - 1].upper_bound;
            count -= buckets[idx - 1].cumulative_count;
            rank -= buckets[idx - 1].cumulative_count;
        }
        start + (end - start) * (rank / count)
    };

    val
}

struct HistogramQuantileIterator {
    inner: Box<dyn InstantSeriesIterator>,
    phi: f64,
    query_points: Vec<i64>,
    is_ready: bool,
    series_group: Vec<InstantSeries>,
}

struct BucketSeries {
    upper_bound: f64,
    samples: Vec<Sample>,
    cursor: usize,
}

impl HistogramQuantileIterator {
    fn new(inner: Box<dyn InstantSeriesIterator>, phi: f64, query_points: Vec<i64>) -> Self {
        Self {
            inner,
            phi,
            query_points,
            is_ready: false,
            series_group: vec![],
        }
    }

    fn load_data(&mut self) -> Result<(), String> {
        if self.is_ready {
            panic!("doule call load data");
        }

        self.is_ready = true;
        let mut store: HashMap<SeriesKey, Vec<BucketSeries>> = HashMap::new();
        let mut le_set: HashMap<SeriesKey, HashSet<String>> = HashMap::new();
        loop {
            if let Some(series) = self.inner.next()? {
                let (le_label, other_labels): (Vec<_>, Vec<_>) =
                    series.labels.into_iter().partition(|l| l.name == "le");
                if le_label.len() != 1 {
                    return Err(format!("must have exactly one le label"));
                }

                let key = SeriesKey::from(other_labels)?;

                let le_label = le_label.first().unwrap();
                if let Some(s) = le_set.get_mut(&key) {
                    if let Some(_) = s.get(&le_label.value) {
                        return Err(format!("found duplicated le value `{}`", le_label.value));
                    } else {
                        s.insert(le_label.value.to_string());
                    }
                } else {
                    let mut s = HashSet::new();
                    s.insert(le_label.value.to_string());
                    le_set.insert(key.clone(), s);
                }

                let upper_bound: f64 = le_label.value.parse().unwrap();
                let series = BucketSeries {
                    upper_bound: upper_bound,
                    samples: series.samples,
                    cursor: 0,
                };
                if let Some(entry) = store.get_mut(&key) {
                    entry.push(series)
                } else {
                    store.insert(key, vec![series]);
                }
            } else {
                break;
            }
        }

        let mut series_group = vec![];
        for (key, buckets) in store.into_iter() {
            let samples = self.evaluate_buckets(buckets);
            series_group.push(InstantSeries::new(key.labels, samples));
        }

        self.series_group = series_group;

        Ok(())
    }

    fn evaluate_buckets(&self, mut series_group: Vec<BucketSeries>) -> Vec<Sample> {
        series_group.sort_by(|left, right| left.upper_bound.total_cmp(&right.upper_bound));

        let mut buckets = Vec::with_capacity(series_group.len());
        let mut output = Vec::with_capacity(self.query_points.len());

        for timestamp in self.query_points.iter() {
            let timestamp = TimestampSecond::new(*timestamp);
            buckets.clear();

            for series in series_group.iter_mut() {
                while series.cursor < series.samples.len()
                    && series.samples[series.cursor].timestamp < timestamp
                {
                    series.cursor += 1;
                }

                let Some(sample) = series.samples.get(series.cursor) else {
                    continue;
                };

                if sample.timestamp == timestamp {
                    buckets.push(Bucket {
                        upper_bound: series.upper_bound,
                        cumulative_count: sample.value,
                    });
                    series.cursor += 1;
                }
            }

            if buckets.is_empty() {
                continue;
            }

            let value = histogram_quantile(self.phi, &mut buckets);
            output.push(Sample::new(timestamp, value));
        }

        output
    }
}

impl InstantSeriesIterator for HistogramQuantileIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        if !self.is_ready {
            self.load_data()?;
            self.is_ready = true;
        }

        Ok(self.series_group.pop())
    }
}

pub fn eval_histogram_quantile(
    mut args: Vec<EvalValue>,
    context: QueryContext,
) -> Result<EvalValue, String> {
    let EvalValue::Scalar(phi) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    let EvalValue::Instant(inner) = args.remove(0) else {
        unreachable!("validated by analyzer");
    };

    if phi < 0.0 || phi > 1.0 {
        return Err(format!(
            "invalid phi: {}, it must between 0 and 1(inclusive)",
            phi,
        ));
    }

    let iter = HistogramQuantileIterator::new(inner, phi, context.query_points);
    Ok(EvalValue::Instant(Box::new(iter)))
}

pub static HISTOGRAM_QUANTILE_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "histogram_quantile",
    arg_types: &[ExpressionType::Scalar, ExpressionType::InstantVector],
    return_type: ExpressionType::InstantVector,
    eval: eval_histogram_quantile,
};

#[cfg(test)]
mod tests {
    use crate::{
        core::{Label, Sample, TimestampSecond},
        query_exec::SeriesListInstantIterator,
    };

    use super::*;

    #[test]
    fn test_histogram_quantile() {
        let buckets = vec![
            Bucket {
                upper_bound: 0.1,
                cumulative_count: 1.0,
            },
            Bucket {
                upper_bound: 0.5,
                cumulative_count: 2.0,
            },
            Bucket {
                upper_bound: 1.0,
                cumulative_count: 3.0,
            },
            Bucket {
                upper_bound: f64::INFINITY,
                cumulative_count: 4.0,
            },
        ];

        let res = histogram_quantile(0.0, &buckets);
        assert_eq!(res, 0.0);

        let res = histogram_quantile(1.0, &buckets);
        assert_eq!(res, 1.0);

        let res = histogram_quantile(0.5, &buckets);
        assert_eq!(res, 0.5);
    }

    #[test]
    fn test_histogram_quantile_with_1_bucket() {
        let buckets = vec![Bucket {
            upper_bound: f64::INFINITY,
            cumulative_count: 4.0,
        }];

        let res = histogram_quantile(0.5, &buckets);
        assert_eq!(res.is_nan(), true);
    }

    #[test]
    fn test_histogram_quantile_miss_inf_bucket() {
        let buckets = vec![
            Bucket {
                upper_bound: 0.1,
                cumulative_count: 1.0,
            },
            Bucket {
                upper_bound: 0.5,
                cumulative_count: 2.0,
            },
            Bucket {
                upper_bound: 1.0,
                cumulative_count: 3.0,
            },
        ];

        let res = histogram_quantile(0.5, &buckets);
        assert_eq!(res.is_nan(), true);
    }

    #[test]
    fn test_histogram_quantitle_function() {
        let series_group = vec!["foo", "bar"]
            .iter()
            .flat_map(|service| {
                let group: Vec<_> = vec!["0.1", "0.5", "1", "+Inf"]
                    .iter()
                    .enumerate()
                    .map(|(idx, le)| {
                        let labels = vec![
                            Label::new("service".to_owned(), service.to_string()),
                            Label::new("le".to_owned(), le.to_string()),
                        ];
                        let samples = (0..2)
                            .map(|i| {
                                let timestamp = TimestampSecond(i);
                                Sample::new(timestamp, i as f64 + idx as f64)
                            })
                            .collect();
                        InstantSeries::new(labels, samples)
                    })
                    .collect();
                group
            })
            .collect();
        let iter = SeriesListInstantIterator {
            series_list: series_group,
        };

        let context = QueryContext::new(vec![0, 1, 2]);
        let res = eval_histogram_quantile(
            vec![EvalValue::Scalar(0.99), EvalValue::Instant(Box::new(iter))],
            context,
        )
        .unwrap();

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
            // TODO: fix test
            // one test for .99, so we can just use the last boundary, while other test case to test the percentile logic itself
            assert_eq!(actual.len(), 2);
            for (_, sample) in actual[0].samples.iter().enumerate() {
                assert_eq!(sample.value, 1.0);
            }
        } else {
            panic!("unexpected eval value");
        }
    }
}
