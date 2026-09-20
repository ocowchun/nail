use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use crate::{
    analyzer::{Analyzer, Binary, Call, InstantSelector, Plan, RangeSelector, SimpleAgg},
    ast::{BinaryOperator, LabelMatcher, SimpleAggregationOperator},
    core::{Label, Labels, Sample, TimestampSecond},
    function::{EvalValue, QueryContext},
    head::{Head, TimeRange},
    lexer::Lexer,
    parser::Parser,
    query_exec::Accumulator::{Avg, Sum},
    request::{QueryRangeRequest, QueryRequest},
};

pub struct QueryExec {
    head: Arc<Head>,
}

pub enum QueryError {
    InvalidRequest(String),
    InternalError(String),
}

impl QueryExec {
    pub fn new(head: Arc<Head>) -> Self {
        Self { head }
    }

    pub fn query(&self, req: QueryRequest) -> Result<QueryResult, QueryError> {
        let lexer = Lexer::new(req.query.clone());
        let mut parser = Parser::new(lexer).map_err(|msg| QueryError::InvalidRequest(msg))?;
        let exp = parser
            .parse()
            .map_err(|msg| QueryError::InvalidRequest(msg))?;
        let analyzer = Analyzer::new();
        let plan = analyzer
            .analyze(&exp)
            .map_err(|msg| QueryError::InvalidRequest(msg))?;

        // TODO: refactor to remove the duplication between query and query_range

        let start = req.time.as_seconds() as i64;
        let query_points = vec![start];

        let mut iter = self
            .evaluate(plan, &query_points)
            .map_err(|msg| QueryError::InternalError(msg))?;

        let mut result = vec![];
        loop {
            let mut series = iter.next().map_err(|msg| QueryError::InternalError(msg))?;
            if let Some(series) = series.as_mut() {
                let sample = series.samples.pop().unwrap();
                // TODO: use move instead of clone
                let labels = series.labels.clone();
                result.push(InstantSample { labels, sample });
            } else {
                break;
            }
        }
        Ok(QueryResult { result })
    }

    pub fn query_range(&self, req: QueryRangeRequest) -> Result<QueryRangeResult, QueryError> {
        let lexer = Lexer::new(req.query.clone());
        let mut parser = Parser::new(lexer).map_err(|msg| QueryError::InvalidRequest(msg))?;
        let exp = parser
            .parse()
            .map_err(|msg| QueryError::InvalidRequest(msg))?;
        let analyzer = Analyzer::new();
        let plan = analyzer
            .analyze(&exp)
            .map_err(|msg| QueryError::InvalidRequest(msg))?;

        let start = req.start.as_seconds() as i64;
        let end = req.end.as_seconds() as i64;
        let step = req.step.as_secs() as i64;
        let sample_count = ((end - start) / step) as usize + 1;

        let query_points: Vec<i64> = (0..sample_count)
            .map(|index| {
                let timestamp = start + index as i64 * step;
                timestamp
            })
            .collect();

        let mut iter = self
            .evaluate(plan, &query_points)
            .map_err(|msg| QueryError::InternalError(msg))?;

        let mut result = vec![];
        loop {
            let series = iter.next().map_err(|msg| QueryError::InternalError(msg))?;
            if let Some(series) = series {
                result.push(series);
            } else {
                break;
            }
        }
        Ok(QueryRangeResult { result })
    }

    // try to run each sub plan for the whole time range, and then emit
    fn evaluate(
        &self,
        plan: Plan,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        match plan {
            Plan::Number(n) => {
                let iter = self.eval_number(n.clone(), query_points)?;
                Ok(iter)
            }
            Plan::String(_str) => {
                todo!();
            }
            Plan::InstantSelector(sel) => {
                let iter = self.eval_instant_selector(sel, query_points)?;
                Ok(iter)
            }
            Plan::RangeSelector(_) => return Err(format!("must be Scalar or instant Vector")),
            Plan::Call(call) => {
                let iter = self.eval_call(call, query_points)?;
                Ok(iter)
            }
            Plan::Binary(binary) => self.eval_binary(binary, &query_points),
            Plan::SimpleAgg(simple_agg) => {
                return self.eval_simple_aggregation(simple_agg, &query_points);
            }
        }
    }

    fn eval_number(
        &self,
        num: f64,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let iter = NumberIterator::new(query_points.clone(), num);
        Ok(Box::new(iter))
    }

    fn eval_instant_selector(
        &self,
        selector: InstantSelector,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let iter = HeadInstantSeriesIterator::new(
            selector.matchers,
            Arc::clone(&self.head),
            query_points.clone(),
        );
        Ok(Box::new(iter))
    }

    fn eval_range_selector(
        &self,
        selector: RangeSelector,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn RangeSeriesIterator>, String> {
        let iter = HeadRangeSeriesIterator::new(
            selector.matchers,
            Arc::clone(&self.head),
            selector.range.clone(),
            query_points.clone(),
        );
        Ok(Box::new(iter))
    }

    fn eval_call(
        &self,
        call: Call,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let mut args = vec![];
        for arg in call.args.into_iter() {
            let arg = match arg {
                Plan::Number(num) => EvalValue::Scalar(num),
                Plan::String(str) => EvalValue::String(str),
                Plan::InstantSelector(instant_selector) => {
                    let iter = self.eval_instant_selector(instant_selector, query_points)?;
                    EvalValue::Instant(iter)
                }
                Plan::RangeSelector(range_selector) => {
                    let iter = self.eval_range_selector(range_selector, query_points)?;
                    EvalValue::Range(iter)
                }
                Plan::Call(call) => {
                    let iter = self.eval_call(call, query_points)?;
                    EvalValue::Instant(iter)
                }
                Plan::Binary(binary) => {
                    let iter = self.eval_binary(binary, query_points)?;
                    EvalValue::Instant(iter)
                }

                Plan::SimpleAgg(simple_agg) => {
                    let iter = self.eval_simple_aggregation(simple_agg, query_points)?;
                    EvalValue::Instant(iter)
                }
            };
            args.push(arg);
        }
        let context = QueryContext::new(query_points.clone());
        let res = (call.spec.eval)(args, context)?;

        return match res {
            EvalValue::Instant(iter) => Ok(iter),
            _ => return Err(format!("unexpedcted eval type")),
        };
    }

    fn eval_binary(
        &self,
        binary: Binary,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let left_iter = self.evaluate(*binary.lhs, query_points)?;
        let right_iter = self.evaluate(*binary.rhs, query_points)?;

        let iter = BinaryIterator::new(binary.op.clone(), left_iter, right_iter);
        Ok(Box::new(iter))
    }

    fn eval_simple_aggregation(
        &self,
        simple_agg: SimpleAgg,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let inner_iter = self.evaluate(*simple_agg.inner_plan, query_points)?;

        let iter = SimpleAggregationIterator::new(
            simple_agg.op,
            inner_iter,
            simple_agg.is_without,
            simple_agg.labels,
        );
        Ok(Box::new(iter))
    }
}

pub struct QueryResult {
    pub result: Vec<InstantSample>,
}

pub struct InstantSample {
    pub labels: Vec<Label>,
    pub sample: Sample,
}

pub struct QueryRangeResult {
    pub result: Vec<InstantSeries>,
}

#[derive(Debug, PartialEq)]
pub struct InstantSeries {
    pub labels: Vec<Label>,
    pub samples: Vec<Sample>,
}

impl InstantSeries {
    pub fn new(labels: Vec<Label>, samples: Vec<Sample>) -> Self {
        Self { labels, samples }
    }
}

pub trait InstantSeriesIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String>;
}

struct NumberIterator {
    query_points: Vec<i64>,
    num: f64,
    finished: bool,
}

impl NumberIterator {
    fn new(query_points: Vec<i64>, num: f64) -> Self {
        Self {
            query_points,
            num,
            finished: false,
        }
    }
}

impl InstantSeriesIterator for NumberIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        if self.finished {
            return Ok(None);
        }

        let samples = self
            .query_points
            .iter()
            .map(|ts| Sample {
                timestamp: TimestampSecond(ts.clone()),
                value: self.num,
            })
            .collect();
        let series = InstantSeries {
            labels: vec![],
            samples,
        };

        self.finished = true;

        Ok(Some(series))
    }
}

struct HeadInstantSeriesIterator {
    series_list: Vec<InstantSeries>,
    matchers: Vec<LabelMatcher>,
    head: Arc<Head>,
    query_points: Vec<i64>,
    is_ready: bool,
}

impl HeadInstantSeriesIterator {
    fn new(matchers: Vec<LabelMatcher>, head: Arc<Head>, query_points: Vec<i64>) -> Self {
        Self {
            series_list: vec![],
            matchers,
            head,
            query_points,
            is_ready: false,
        }
    }

    fn load_data(&mut self) -> Result<(), String> {
        if self.is_ready {
            panic!("doule call load data");
        }

        // 5 min
        let lookback_period = 5 * 3600;
        let start = 0.max(self.query_points.first().unwrap() - lookback_period);

        let range = TimeRange {
            start: TimestampSecond(start),
            end: TimestampSecond(self.query_points.last().unwrap().clone()),
        };
        let query_series = self.head.query_range(&self.matchers, range)?;
        let mut iter = query_series.iter();

        let mut series_list = vec![];
        while let Some(series) = iter.next() {
            let mut cursor = 0;
            let mut samples = vec![];

            let mut candidate = None;

            for query_point in self.query_points.iter() {
                while cursor < series.samples.len()
                    && series.samples[cursor].timestamp <= TimestampSecond(*query_point)
                {
                    candidate = Some(series.samples[cursor]);
                    cursor += 1;
                }
                let threshold = if *query_point >= lookback_period {
                    TimestampSecond(0)
                } else {
                    TimestampSecond(*query_point - lookback_period)
                };
                if let Some(sample) = candidate
                    && sample.timestamp >= threshold
                {
                    samples.push(Sample {
                        timestamp: TimestampSecond(query_point.clone()),
                        value: sample.value,
                    });
                }
            }
            series_list.push(InstantSeries {
                labels: series.labels.clone(),
                samples: samples,
            });
        }
        self.series_list = series_list;
        Ok(())
    }
}

impl InstantSeriesIterator for HeadInstantSeriesIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        if !self.is_ready {
            self.load_data()?;
            self.is_ready = true;
        }

        Ok(self.series_list.pop())
    }
}

struct BinaryIterator {
    op: BinaryOperator,
    left_iter: Box<dyn InstantSeriesIterator>,
    right_iter: Box<dyn InstantSeriesIterator>,
    current_left_series: Option<InstantSeries>,
    right_series: Vec<InstantSeries>,
    next_left: bool,
    right_iter_finished: bool,
    right_cursor: usize,
}

impl BinaryIterator {
    fn new(
        op: BinaryOperator,
        left_iter: Box<dyn InstantSeriesIterator>,
        right_iter: Box<dyn InstantSeriesIterator>,
    ) -> Self {
        Self {
            op,
            left_iter,
            right_iter,
            current_left_series: None,
            right_series: vec![],
            next_left: true,
            right_iter_finished: false,
            right_cursor: 0,
        }
    }

    fn eval(
        &self,
        left: &InstantSeries,
        right: &InstantSeries,
    ) -> Result<Option<InstantSeries>, String> {
        // TODO: handle label match
        if left.labels != right.labels {
            return Ok(None);
        }

        let mut right_cursor: usize = 0;

        let mut samples = vec![];
        for left_sample in left.samples.iter() {
            while right_cursor < right.samples.len()
                && left_sample.timestamp > right.samples[right_cursor].timestamp
            {
                right_cursor += 1;
            }
            if right_cursor >= right.samples.len() {
                break;
            }

            if left_sample.timestamp == right.samples[right_cursor].timestamp {
                let right_sample = right.samples[right_cursor];

                let value = match self.op {
                    BinaryOperator::Plus => left_sample.value + right_sample.value,
                    BinaryOperator::Minus => left_sample.value - right_sample.value,
                    BinaryOperator::Multiply => left_sample.value * right_sample.value,
                    BinaryOperator::Divide => {
                        // handle divide by zero
                        left_sample.value / right_sample.value
                    }
                    _ => {
                        todo!()
                    }
                };
                samples.push(Sample {
                    timestamp: left_sample.timestamp.clone(),
                    value,
                });
            }
        }

        Ok(Some(InstantSeries {
            labels: left.labels.clone(),
            samples,
        }))
    }
}

impl InstantSeriesIterator for BinaryIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        loop {
            if self.next_left {
                self.current_left_series = self.left_iter.next()?;
                self.next_left = false;
                self.right_cursor = 0;
            }
            if !self.right_iter_finished {
                let series = self.right_iter.next()?;
                match series {
                    Some(series) => {
                        self.right_series.push(series);
                    }
                    None => {
                        self.right_iter_finished = true;
                    }
                };
            }

            let right_series = self.right_series.get(self.right_cursor).unwrap();
            self.right_cursor += 1;
            if self.right_iter_finished && self.right_cursor == self.right_series.len() {
                self.next_left = true;
            }

            let result = match &self.current_left_series {
                Some(left_series) => self.eval(left_series, right_series),
                None => {
                    self.next_left = false;
                    return Ok(None);
                }
            };
            match result {
                Ok(res) => match res {
                    Some(series) => {
                        return Ok(Some(series));
                    }
                    None => {}
                },
                Err(err) => {
                    return Err(err);
                }
            }
        }
    }
}

struct SimpleAggregationIterator {
    op: SimpleAggregationOperator,
    inner_iter: Box<dyn InstantSeriesIterator>,
    is_without: bool,
    labels: Vec<String>,
    state: SimpleAggregationIteratorState,
    series_list: Vec<InstantSeries>,
}

#[derive(PartialEq, Debug, Clone, Eq)]
enum SimpleAggregationIteratorState {
    Uninitialized,
    Ready,
    Done,
}

enum Accumulator {
    Sum(f64),
    Avg {
        sum: f64,
        count: u64,
    },
    Min(f64),
    Max(f64),
    Count(u64),
    Group,
    Stddev {
        sum: f64,
        sqaure_sum: f64,
        count: u64,
    },
    Stdvar {
        sum: f64,
        sqaure_sum: f64,
        count: u64,
    },
}

impl Accumulator {
    fn new(op: &SimpleAggregationOperator, value: f64) -> Self {
        match op {
            SimpleAggregationOperator::Sum => Self::Sum(value),
            SimpleAggregationOperator::Avg => Self::Avg {
                sum: value,
                count: 1,
            },
            SimpleAggregationOperator::Min => Self::Min(f64::MIN),
            SimpleAggregationOperator::Max => Self::Max(f64::MAX),
            SimpleAggregationOperator::Group => Self::Group,
            SimpleAggregationOperator::Count => Self::Sum(0.0),
            SimpleAggregationOperator::Stddev => Self::Stddev {
                sum: value,
                sqaure_sum: value * value,
                count: 1,
            },
            SimpleAggregationOperator::Stdvar => Self::Stdvar {
                sum: value,
                sqaure_sum: value * value,
                count: 1,
            },
        }
    }

    fn observe(&mut self, sample: f64) {
        match self {
            Self::Sum(sum) => *sum += sample,
            Self::Avg { sum, count } => {
                *sum += sample;
                *count += 1;
            }
            Self::Min(min) => *min = min.min(sample),
            Self::Max(max) => *max = max.max(sample),
            Self::Count(count) => *count += 1,
            Self::Group => {}
            Self::Stddev {
                sum,
                sqaure_sum,
                count,
            } => {
                *sum += sample;
                *sqaure_sum += sample * sample;
                *count += 1;
            }
            Self::Stdvar {
                sum,
                sqaure_sum,
                count,
            } => {
                *sum += sample;
                *sqaure_sum += sample * sample;
                *count += 1;
            }
        }
    }

    fn value(&self) -> f64 {
        match self {
            Sum(sum) => *sum,
            Avg { sum, count } => *sum / *count as f64,
            Accumulator::Min(min) => *min,
            Accumulator::Max(max) => *max,
            Accumulator::Count(count) => *count as f64,
            Accumulator::Group => 1.0,
            Self::Stddev {
                sum,
                sqaure_sum,
                count,
            } => {
                let mu = *sum / *count as f64;
                let var =
                    *sqaure_sum - (2.0 * mu * (*sum)) + (*count as f64 * mu * mu) / (*count as f64);
                f64::sqrt(var)
            }
            Self::Stdvar {
                sum,
                sqaure_sum,
                count,
            } => {
                let mu = *sum / *count as f64;
                let var =
                    *sqaure_sum - (2.0 * mu * (*sum)) + (*count as f64 * mu * mu) / (*count as f64);
                var
            }
        }
    }
}

impl SimpleAggregationIterator {
    fn new(
        op: SimpleAggregationOperator,
        inner_iter: Box<dyn InstantSeriesIterator>,
        is_without: bool,
        labels: Vec<String>,
    ) -> Self {
        Self {
            op,
            inner_iter,
            is_without,
            labels,
            state: SimpleAggregationIteratorState::Uninitialized,
            series_list: vec![],
        }
    }

    fn drain_and_aggregate(&mut self) -> Result<(), String> {
        let mut store: HashMap<Labels, BTreeMap<TimestampSecond, Accumulator>> = HashMap::new();

        loop {
            let entry = self.inner_iter.next()?;
            if let Some(series) = entry {
                let labels = self.compute_labels(series.labels)?;
                if let Some(map) = store.get_mut(&labels) {
                    for sample in series.samples.into_iter() {
                        let key = sample.timestamp;
                        map.entry(key)
                            .and_modify(|acc| acc.observe(sample.value))
                            .or_insert_with(|| Accumulator::new(&self.op, sample.value));
                    }
                } else {
                    let mut sub_map: BTreeMap<TimestampSecond, Accumulator> = BTreeMap::new();
                    for sample in series.samples.into_iter() {
                        let key = sample.timestamp;
                        sub_map.insert(key, Accumulator::new(&self.op, sample.value));
                    }
                    store.insert(labels, sub_map);
                }
            } else {
                break;
            }
        }

        // let mut series
        self.series_list = store
            .into_iter()
            .map(|(labels, sub_map)| {
                let samples: Vec<_> = sub_map
                    .into_iter()
                    .map(|(timestamp, acc)| Sample {
                        timestamp,
                        value: acc.value(),
                    })
                    .collect();

                InstantSeries {
                    labels: labels.labels(),
                    samples: samples,
                }
            })
            .collect();
        self.state = SimpleAggregationIteratorState::Ready;
        Ok(())
    }

    fn compute_labels(&self, labels: Vec<Label>) -> Result<Labels, String> {
        // `without` removes the listed labels from the result vector,
        // while all other labels are preserved in the output.
        // `by` does the opposite and drops labels that are not listed in the by clause,
        // even if their label values are identical between all elements of the vector.
        let filtered: Vec<_> = if self.is_without {
            labels
                .into_iter()
                .filter(|label| !self.labels.contains(&label.name))
                .collect()
        } else {
            labels
                .into_iter()
                .filter(|label| self.labels.contains(&label.name))
                .collect()
        };

        let labels = Labels::from(filtered)?;
        Ok(labels)
    }
}

impl InstantSeriesIterator for SimpleAggregationIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        if self.state == SimpleAggregationIteratorState::Uninitialized {
            self.drain_and_aggregate()?;
        }

        match &self.state {
            SimpleAggregationIteratorState::Uninitialized => unreachable!(),
            SimpleAggregationIteratorState::Ready => match self.series_list.pop() {
                Some(series) => Ok(Some(series)),
                None => {
                    self.state = SimpleAggregationIteratorState::Done;
                    Ok(None)
                }
            },
            SimpleAggregationIteratorState::Done => Ok(None),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeSeries {
    pub labels: Vec<Label>,
    pub samples: Vec<RangeSample>,
}

impl RangeSeries {
    pub fn new(labels: Vec<Label>, samples: Vec<RangeSample>) -> Self {
        Self { labels, samples }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeSample {
    pub range: TimeRange,
    pub samples: Vec<Sample>,
}

impl RangeSample {
    pub fn new(range: TimeRange, samples: Vec<Sample>) -> Self {
        Self { range, samples }
    }
}

pub trait RangeSeriesIterator {
    fn next(&mut self) -> Result<Option<RangeSeries>, String>;
}

struct HeadRangeSeriesIterator {
    head: Arc<Head>,
    duration: Duration,
    matchers: Vec<LabelMatcher>,
    // range: TimeRange,
    query_points: VecDeque<i64>,
    series_list: Vec<RangeSeries>,
    is_ready: bool,
}

impl HeadRangeSeriesIterator {
    pub fn new(
        matchers: Vec<LabelMatcher>,
        head: Arc<Head>,
        duration: Duration,
        query_points: Vec<i64>,
    ) -> Self {
        let query_points = VecDeque::from(query_points);
        Self {
            head,
            duration,
            matchers,
            query_points,
            series_list: vec![],
            is_ready: false,
        }
    }

    fn load_data(&mut self) {
        if self.is_ready {
            panic!("double call load_data")
        }

        let mut store: HashMap<Vec<Label>, Vec<RangeSample>> = HashMap::new();
        loop {
            if let Some(end) = self.query_points.pop_front() {
                let start = if end > self.duration.as_secs() as i64 {
                    end - self.duration.as_secs() as i64
                } else {
                    0
                };
                let range = TimeRange {
                    start: TimestampSecond(start),
                    end: TimestampSecond(end),
                };
                let res = self
                    .head
                    .query_range(&self.matchers, range.clone())
                    .unwrap();

                for series in res.into_iter() {
                    let sample = RangeSample {
                        range: range.clone(),
                        samples: series.samples,
                    };
                    if let Some(entry) = store.get_mut(&series.labels) {
                        entry.push(sample);
                    } else {
                        store.insert(series.labels, vec![sample]);
                    }
                }
            } else {
                break;
            }
        }

        self.series_list = store
            .into_iter()
            .map(|(labels, samples)| RangeSeries { labels, samples })
            .collect();
    }
}

impl RangeSeriesIterator for HeadRangeSeriesIterator {
    fn next(&mut self) -> Result<Option<RangeSeries>, String> {
        if !self.is_ready {
            self.load_data();
        }
        self.is_ready = true;

        if let Some(series) = self.series_list.pop() {
            Ok(Some(series))
        } else {
            Ok(None)
        }
    }
}

pub struct SeriesListInstantIterator {
    pub series_list: Vec<InstantSeries>,
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

pub struct SeriesListRangeIterator {
    series_list: Vec<RangeSeries>,
}

impl SeriesListRangeIterator {
    pub fn new(series_list: Vec<RangeSeries>) -> Self {
        Self { series_list }
    }
}

impl RangeSeriesIterator for SeriesListRangeIterator {
    fn next(&mut self) -> Result<Option<RangeSeries>, String> {
        if let Some(series) = self.series_list.pop() {
            return Ok(Some(series));
        }
        return Ok(None);
    }
}

#[cfg(test)]
mod tests {
    use crate::core::SeriesKey;

    use super::*;

    #[test]
    fn test_number_iterator() {
        let num: f64 = 1.23;
        let query_points = vec![1, 10, 20, 30];
        let mut iter = NumberIterator::new(query_points.clone(), num);

        let expected_samples: Vec<Sample> = query_points
            .iter()
            .map(|ts| Sample {
                timestamp: TimestampSecond(ts.clone()),
                value: num.clone(),
            })
            .collect();

        let series = iter.next().unwrap().unwrap();
        if let Some(_) = iter.next().unwrap() {
            assert!(false, "it should only return one series");
        }
        assert_eq!(series.labels.len(), 0);
        for (index, expected_sample) in expected_samples.iter().enumerate() {
            let sample = series.samples[index];
            assert_eq!(&sample, expected_sample);
        }
    }

    #[test]
    fn test_head_instant_selector_iterator() {
        let h = Arc::new(Head::new(8));
        (0..10).for_each(|i: i32| {
            vec!["foo", "bar"].iter().for_each(|service| {
                let labels = vec![
                    Label::new(format!("__name__"), format!("dummy_counter")),
                    Label::new(format!("service"), service.to_string()),
                ];

                let series_key = SeriesKey::from(labels).unwrap();

                let sample = Sample {
                    timestamp: TimestampSecond(i64::from(i)),
                    value: f64::from(i),
                };

                h.append(series_key, sample).unwrap();
            });
        });
        let matchers = vec![
            LabelMatcher::new(
                crate::ast::LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "dummy_counter".to_string(),
            ),
            LabelMatcher::new(
                crate::ast::LabelMatcherOperator::Equal,
                "service".to_string(),
                "bar".to_string(),
            ),
        ];
        let query_points = vec![1, 4, 7];

        let mut iter =
            HeadInstantSeriesIterator::new(matchers, Arc::clone(&h), query_points.clone());

        let series = iter.next().unwrap().unwrap();
        let expected_labels = vec![
            Label::new("__name__".to_string(), "dummy_counter".to_string()),
            Label::new("service".to_string(), "bar".to_string()),
        ];
        assert_eq!(series.labels, expected_labels);
        let expected_samples: Vec<Sample> = query_points
            .iter()
            .map(|ts| Sample {
                timestamp: TimestampSecond(ts.clone()),
                value: ts.clone() as f64,
            })
            .collect();
        assert_eq!(series.samples, expected_samples);
    }

    #[test]
    fn test_head_range_selector_iterator() {
        let h = Arc::new(Head::new(8));
        (0..10).for_each(|i: i32| {
            vec!["foo", "bar"].iter().for_each(|service| {
                let labels = vec![
                    Label::new(format!("__name__"), format!("dummy_counter")),
                    Label::new(format!("service"), service.to_string()),
                ];

                let series_key = SeriesKey::from(labels).unwrap();

                let sample = Sample {
                    timestamp: TimestampSecond(i64::from(100 + i)),
                    value: f64::from(i),
                };

                h.append(series_key, sample).unwrap();
            });
        });
        let matchers = vec![
            LabelMatcher::new(
                crate::ast::LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "dummy_counter".to_string(),
            ),
            LabelMatcher::new(
                crate::ast::LabelMatcherOperator::Equal,
                "service".to_string(),
                "bar".to_string(),
            ),
        ];
        let query_points = vec![102, 105];
        let duration = Duration::from_secs(5);

        let mut iter =
            HeadRangeSeriesIterator::new(matchers, Arc::clone(&h), duration, query_points);

        let series = iter.next().unwrap().unwrap();
        let expected_labels = vec![
            Label::new("__name__".to_string(), "dummy_counter".to_string()),
            Label::new("service".to_string(), "bar".to_string()),
        ];
        assert_eq!(series.labels, expected_labels);
        let expected_samples = vec![
            RangeSample {
                range: TimeRange {
                    start: TimestampSecond(97),
                    end: TimestampSecond(102),
                },
                samples: vec![
                    Sample {
                        timestamp: TimestampSecond(100),
                        value: 0.0,
                    },
                    Sample {
                        timestamp: TimestampSecond(101),
                        value: 1.0,
                    },
                    Sample {
                        timestamp: TimestampSecond(102),
                        value: 2.0,
                    },
                ],
            },
            RangeSample {
                range: TimeRange {
                    start: TimestampSecond(100),
                    end: TimestampSecond(105),
                },
                samples: vec![
                    Sample {
                        timestamp: TimestampSecond(101),
                        value: 1.0,
                    },
                    Sample {
                        timestamp: TimestampSecond(102),
                        value: 2.0,
                    },
                    Sample {
                        timestamp: TimestampSecond(103),
                        value: 3.0,
                    },
                    Sample {
                        timestamp: TimestampSecond(104),
                        value: 4.0,
                    },
                    Sample {
                        timestamp: TimestampSecond(105),
                        value: 5.0,
                    },
                ],
            },
        ];
        assert_eq!(series.samples, expected_samples);
    }

    #[test]
    fn test_simple_agg_iterator() {
        let mut series_list = vec![];
        vec!["foo", "bar"].into_iter().for_each(|service| {
            let samples: Vec<_> = (0..3)
                .into_iter()
                .map(|i| Sample::new(TimestampSecond::new(i as i64), i as f64))
                .collect();
            series_list.push(InstantSeries {
                labels: vec![
                    Label::new(format!("__name__"), format!("dummy_counter")),
                    Label::new(format!("service"), service.to_string()),
                    Label::new(format!("pod"), format!("pod1")),
                ],
                samples: samples.clone(),
            });
            series_list.push(InstantSeries {
                labels: vec![
                    Label::new(format!("__name__"), format!("dummy_counter")),
                    Label::new(format!("service"), service.to_string()),
                    Label::new(format!("pod"), format!("pod1")),
                ],
                samples: samples.clone(),
            });
        });
        let inner_iter = SeriesListInstantIterator::new(series_list);

        let mut iter = SimpleAggregationIterator::new(
            SimpleAggregationOperator::Sum,
            Box::new(inner_iter),
            false,
            vec![format!("service")],
        );

        let mut actual_series_list = vec![];
        loop {
            if let Some(series) = iter.next().unwrap() {
                actual_series_list.push(series);
            } else {
                break;
            }
        }
        actual_series_list.sort_by(|left, right| {
            left.labels
                .first()
                .unwrap()
                .value
                .cmp(&right.labels.first().unwrap().value)
        });

        let expected_series_list = vec![
            InstantSeries::new(
                vec![Label::new(format!("service"), format!("bar"))],
                vec![
                    Sample::new(TimestampSecond(0), 0.0),
                    Sample::new(TimestampSecond(1), 2.0),
                    Sample::new(TimestampSecond(2), 4.0),
                ],
            ),
            InstantSeries::new(
                vec![Label::new(format!("service"), format!("foo"))],
                vec![
                    Sample::new(TimestampSecond(0), 0.0),
                    Sample::new(TimestampSecond(1), 2.0),
                    Sample::new(TimestampSecond(2), 4.0),
                ],
            ),
        ];
        assert_eq!(actual_series_list, expected_series_list);
    }
}
