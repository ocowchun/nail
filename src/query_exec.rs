use std::sync::Arc;

use crate::{
    analyzer::{Analyzer, Binary, Call, InstantSelector, Plan},
    ast::BinaryOperator,
    core::Label,
    head::{Head, QuerySeries, Sample, TimeRange, TimestampSecond},
    lexer::Lexer,
    parser::Parser,
    request::{QueryRangeRequest, QueryRequest},
};

pub struct QueryExec {
    head: Arc<Head>,
}

impl QueryExec {
    pub fn new(head: Arc<Head>) -> Self {
        Self { head }
    }

    pub fn query(&self, req: QueryRequest) -> Result<QueryResult, String> {
        let lexer = Lexer::new(req.query.clone());
        let mut parser = Parser::new(lexer);
        let exp = parser.parse()?;
        let analyzer = Analyzer::new();
        let plan = analyzer.analyze(&exp)?;

        // TODO: refactor to remove the duplication between query and query_range

        let start = req.time.as_seconds() as i64;
        let query_points = vec![start];

        let mut iter = self.evaluate(&plan, &query_points)?;

        let mut result = vec![];
        loop {
            let mut series = iter.next()?;
            if let Some(series) = series.as_mut() {
                println!("query: add sample");
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

    pub fn query_range(&self, req: QueryRangeRequest) -> Result<QueryRangeResult, String> {
        let lexer = Lexer::new(req.query.clone());
        let mut parser = Parser::new(lexer);
        let exp = parser.parse()?;
        let analyzer = Analyzer::new();
        let plan = analyzer.analyze(&exp)?;

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

        let mut iter = self.evaluate(&plan, &query_points)?;

        let mut result = vec![];
        loop {
            let series = iter.next()?;
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
        plan: &Plan,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        match plan {
            Plan::Number(n) => {
                let iter = self.eval_number(n.clone(), query_points)?;
                Ok(iter)
            }
            Plan::String(str) => {
                todo!();
            }
            Plan::InstantSelector(sel) => {
                let iter = self.eval_instant_selector(sel, query_points)?;
                Ok(iter)
            }
            Plan::RangeSelector(_) => return Err(format!("must be Scalar or instant Vector")),
            Plan::Call(call) => {
                todo!();
            }
            Plan::Binary(binary) => self.eval_binary(&binary, &query_points),
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
        selector: &InstantSelector,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let matchers = &selector.matchers;
        let range = TimeRange {
            start: TimestampSecond(query_points.first().unwrap().clone()),
            end: TimestampSecond(query_points.last().unwrap().clone()),
        };

        let query_series = self.head.query_range(matchers, range);
        let iter = SeriesIterator::new(query_series, query_points.clone());
        Ok(Box::new(iter))
    }

    fn eval_call(&self, call: &Call, query_points: &Vec<i64>) -> Result<InstantSeries, String> {
        todo!()
    }

    fn eval_binary(
        &self,
        binary: &Binary,
        query_points: &Vec<i64>,
    ) -> Result<Box<dyn InstantSeriesIterator>, String> {
        let left_iter = self.evaluate(&binary.lhs, query_points)?;
        let right_iter = self.evaluate(&binary.rhs, query_points)?;

        let iter = BinaryIterator::new(
            query_points.clone(),
            binary.op.clone(),
            left_iter,
            right_iter,
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

pub struct InstantSeries {
    pub labels: Vec<Label>,
    pub samples: Vec<Sample>,
}

trait InstantSeriesIterator {
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

struct SeriesIterator {
    series_list: Vec<InstantSeries>,
    cursor: usize,
}

impl SeriesIterator {
    fn new(query_series: Vec<QuerySeries>, query_points: Vec<i64>) -> Self {
        // 5 min
        let loopback_period = 5 * 3600;
        // let loopback_period = 0;
        let mut iter = query_series.iter();

        let mut series_list = vec![];
        while let Some(series) = iter.next() {
            let mut cursor = 0;
            let mut samples = vec![];

            let mut candidate = None;

            println!("series len -> {}", series.samples.len());
            for query_point in query_points.iter() {
                // println!("target_timestamp -> {}", target_timestamp.0);
                while cursor < series.samples.len()
                    && series.samples[cursor].timestamp <= TimestampSecond(*query_point)
                {
                    println!("sample {}", series.samples[cursor].timestamp.0);
                    candidate = Some(series.samples[cursor]);
                    cursor += 1;
                }
                let threshold = if (*query_point >= loopback_period) {
                    TimestampSecond(0)
                } else {
                    TimestampSecond(*query_point - loopback_period)
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

        Self {
            series_list,
            cursor: 0,
        }
    }
}

impl InstantSeriesIterator for SeriesIterator {
    fn next(&mut self) -> Result<Option<InstantSeries>, String> {
        if self.series_list.is_empty() {
            return Ok(None);
        }

        let series = self.series_list.pop().unwrap();
        return Ok(Some(series));
    }
}

struct BinaryIterator {
    query_points: Vec<i64>,
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
        query_points: Vec<i64>,
        op: BinaryOperator,
        left_iter: Box<dyn InstantSeriesIterator>,
        right_iter: Box<dyn InstantSeriesIterator>,
    ) -> Self {
        Self {
            query_points,
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

struct RangeSeries {
    labels: Vec<Label>,
    samples: Vec<Vec<Sample>>,
}

trait RangeSeriesIterator {
    fn next(&mut self) -> Result<Option<RangeSeries>, String>;
}

#[cfg(test)]
mod tests {
    use crate::head::SeriesKey;

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
    fn test_instant_selector_iterator() {
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

        // InstantSelectorIterator::new(query_series, query_points)

        // TODO
        // let num: f64 = 1.23;
        // let query_points = vec![1, 10, 20, 30];
        // let mut iter = NumberIterator::new(query_points.clone(), num);

        // let expected_samples: Vec<Sample> = query_points
        //     .iter()
        //     .map(|ts| Sample {
        //         timestamp: TimestampMillis(ts.clone()),
        //         value: num.clone(),
        //     })
        //     .collect();

        // let series = iter.next().unwrap().unwrap();
        // if let Some(_) = iter.next().unwrap() {
        //     assert!(false, "it should only return one series");
        // }
        // assert_eq!(series.labels.len(), 0);
        // for (index, expected_sample) in expected_samples.iter().enumerate() {
        //     let sample = series.samples[index];
        //     assert_eq!(&sample, expected_sample);
        // }
    }
}
