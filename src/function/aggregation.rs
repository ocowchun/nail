use crate::{
    core::Sample,
    query_exec::{InstantSeries, InstantSeriesIterator, RangeSample, RangeSeriesIterator},
};

enum SimpleOverTimeAggregationOperator {
    Avg,
    Min,
    Max,
    Sum,
    Count,
    Stddev,
    Stdvar,
    Last,
    First,
    Present,
}

struct SimpleOverTimeAggregationIterator {
    op: SimpleOverTimeAggregationOperator,
    inner: Box<dyn RangeSeriesIterator>,
    is_ready: bool,
}

fn aggregation(
    range_sample: RangeSample,
    op: &SimpleOverTimeAggregationOperator,
) -> Option<Sample> {
    if range_sample.samples.is_empty() {
        return None;
        // TODO: do something
    }

    let n = range_sample.samples.len() as f64;
    let value = match op {
        SimpleOverTimeAggregationOperator::Avg => {
            range_sample
                .samples
                .into_iter()
                .fold(0.0, |acc, sample| acc + sample.value)
                / n
        }
        SimpleOverTimeAggregationOperator::Min => {
            range_sample
                .samples
                .iter()
                .min_by(|l, r| l.value.total_cmp(&r.value))
                .unwrap()
                .value
        }
        SimpleOverTimeAggregationOperator::Max => {
            range_sample
                .samples
                .iter()
                .max_by(|l, r| l.value.total_cmp(&r.value))
                .unwrap()
                .value
        }
        SimpleOverTimeAggregationOperator::Sum => range_sample
            .samples
            .into_iter()
            .fold(0.0, |acc, sample| acc + sample.value),
        SimpleOverTimeAggregationOperator::Count => n,
        SimpleOverTimeAggregationOperator::Stddev => {
            compute_standard_deviation(range_sample.samples).stddev
        }
        SimpleOverTimeAggregationOperator::Stdvar => {
            compute_standard_deviation(range_sample.samples).stdvar
        }
        SimpleOverTimeAggregationOperator::Last => range_sample.samples.last().unwrap().value,
        SimpleOverTimeAggregationOperator::First => range_sample.samples.first().unwrap().value,
        SimpleOverTimeAggregationOperator::Present => {
            // TODO: confirm should we return 0 if no sample
            1.0
        }
    };

    Some(Sample::new(range_sample.range.end, value))
}

struct StandardDeviation {
    stddev: f64,
    stdvar: f64,
}

fn compute_standard_deviation(samples: Vec<Sample>) -> StandardDeviation {
    let mut sum = 0.0;
    let mut square_sum = 0.0;
    let count = samples.len() as f64;

    for sample in samples.into_iter() {
        sum += sample.value;
        square_sum += sample.value * sample.value;
    }

    let mu = sum / count;
    let var = square_sum - (2.0 * mu * (sum)) + (count as f64 * mu * mu) / (count as f64);
    StandardDeviation {
        stddev: f64::sqrt(var),
        stdvar: var,
    }
}

impl InstantSeriesIterator for SimpleOverTimeAggregationIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        if let Some(series) = self.inner.next()? {
            let mut samples = vec![];
            for range_sample in series.samples.into_iter() {
                if let Some(sample) = aggregation(range_sample, &self.op) {
                    samples.push(sample);
                }
            }

            Ok(Some(InstantSeries::new(series.labels, samples)))
        } else {
            Ok(None)
        }
    }
}
