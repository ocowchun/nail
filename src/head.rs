use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
    sync::{RwLock, atomic::AtomicU64},
};

use regex::Regex;

use crate::{
    ast::{LabelMatcher, LabelMatcherOperator},
    core::Label,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampSecond(pub i64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub timestamp: TimestampSecond,
    pub value: f64,
}
impl Sample {
    pub fn new(timestamp: TimestampSecond, value: f64) -> Self {
        Self { timestamp, value }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeriesKey {
    labels: Vec<Label>,
}

impl SeriesKey {
    pub fn from(mut labels: Vec<Label>) -> Result<Self, String> {
        labels.sort_by(|left, right| left.name.cmp(&right.name));
        for pair in labels.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(format!("duplicate label name {}", pair[0].name));
            }
        }

        Ok(Self { labels })
    }
}

#[derive(Debug)]
struct Series {
    id: SeriesId,
    key: SeriesKey,
    samples: Vec<Sample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start: TimestampSecond,
    pub end: TimestampSecond,
}

impl TimeRange {
    pub fn new(start: TimestampSecond, end: TimestampSecond) -> Self {
        Self { start, end }
    }
}

impl Series {
    fn new(id: SeriesId, key: SeriesKey) -> Self {
        Self {
            id,
            key,
            samples: vec![],
        }
    }

    fn append(&mut self, sample: Sample) -> Result<(), String> {
        if let Some(last) = self.samples.last() {
            if sample.timestamp <= last.timestamp {
                return Err(format!("new sample.timestamp must > last sample.timestamp"));
            }
        }

        self.samples.push(sample);

        Ok(())
    }

    fn query_samples(&self, range: &TimeRange) -> &[Sample] {
        // Returns samples in the left-open, right-closed interval `(start, end]`.
        let start = self
            .samples
            .partition_point(|sample| sample.timestamp <= range.start);
        let end = self
            .samples
            .partition_point(|sample| sample.timestamp <= range.end);
        &self.samples[start..end]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeriesId(u64);

type Postings = Vec<SeriesId>;

#[derive(Debug, Default)]
pub struct Head {
    shards: Vec<RwLock<Shard>>,
    next_series_id: AtomicU64,
}

#[derive(Debug, Default)]
struct Shard {
    series_by_key: HashMap<SeriesKey, SeriesId>,
    series_by_id: HashMap<SeriesId, Series>,

    // lable_name -> label_value -> SeriesIds
    postings: HashMap<String, BTreeMap<String, Vec<SeriesId>>>,
}

impl Shard {
    fn new() -> Self {
        Self {
            series_by_key: HashMap::new(),
            series_by_id: HashMap::new(),
            postings: HashMap::new(),
        }
    }

    fn create_series(&mut self, id: SeriesId, key: SeriesKey) {
        for label in &key.labels {
            self.postings
                .entry(label.name.clone())
                .or_default()
                .entry(label.value.clone())
                .or_default()
                .push(id);
        }

        self.series_by_key.insert(key.clone(), id);
        self.series_by_id.insert(id, Series::new(id, key));
    }

    fn query_postings_for_equal(&self, matcher: &LabelMatcher) -> Vec<SeriesId> {
        match self.postings.get(&matcher.label_name) {
            Some(values) => values
                .get(&matcher.label_value)
                .map(|v| v.clone())
                .unwrap_or(vec![]),
            None => {
                vec![]
            }
        }
    }

    fn query_postings_for_not_equal(&self, matcher: &LabelMatcher) -> Vec<SeriesId> {
        match self.postings.get(&matcher.label_name) {
            Some(values) => {
                let vectors = values
                    .iter()
                    .filter(|(k, _)| (*k).ne(&matcher.label_value))
                    .map(|(_, v)| v.clone())
                    .collect();
                Self::merge_sorted(vectors)
            }
            None => {
                vec![]
            }
        }
    }

    fn query_postings_for_regex_match(
        &self,
        matcher: &LabelMatcher,
    ) -> Result<Vec<SeriesId>, String> {
        let re = Regex::new(&matcher.label_value).map_err(|err| err.to_string())?;

        let res = match self.postings.get(&matcher.label_name) {
            Some(values) => {
                let vectors = values
                    .iter()
                    .filter(|(k, _)| re.is_match(*k))
                    .map(|(_, v)| v.clone())
                    .collect();
                Self::merge_sorted(vectors)
            }
            None => {
                vec![]
            }
        };
        Ok(res)
    }

    fn merge_sorted(vectors: Vec<Vec<SeriesId>>) -> Vec<SeriesId> {
        let total_len: usize = vectors.iter().map(Vec::len).sum();
        let mut result = Vec::with_capacity(total_len);

        let mut heap = BinaryHeap::new();
        for (vec_index, v) in vectors.iter().enumerate() {
            if let Some(&value) = v.first() {
                heap.push(Reverse((value, vec_index, 0)));
            }
        }

        while let Some(Reverse((value, vec_index, elem_index))) = heap.pop() {
            result.push(value);

            let next_index = elem_index + 1;
            if let Some(&next_value) = vectors[vec_index].get(next_index) {
                heap.push(Reverse((next_value, vec_index, next_index)));
            }
        }

        result
    }

    fn query_postings_for_not_regex_match(
        &self,
        matcher: &LabelMatcher,
    ) -> Result<Vec<SeriesId>, String> {
        let re = Regex::new(&matcher.label_value).map_err(|err| err.to_string())?;

        let res = match self.postings.get(&matcher.label_name) {
            Some(values) => {
                let vectors = values
                    .iter()
                    .filter(|(k, _)| !re.is_match(*k))
                    .map(|(_, v)| v.clone())
                    .collect();
                Self::merge_sorted(vectors)
            }
            None => {
                vec![]
            }
        };
        Ok(res)
    }

    fn query_postings(&self, matcher: &LabelMatcher) -> Result<Vec<SeriesId>, String> {
        match matcher.operator {
            LabelMatcherOperator::Equal => Ok(self.query_postings_for_equal(matcher)),
            LabelMatcherOperator::NotEqual => Ok(self.query_postings_for_not_equal(matcher)),
            LabelMatcherOperator::RegexMatch => self.query_postings_for_regex_match(matcher),
            LabelMatcherOperator::NotRegexMatch => self.query_postings_for_not_regex_match(matcher),
        }
    }

    fn select(&self, matchers: &[LabelMatcher]) -> Result<Vec<SeriesId>, String> {
        if matchers.is_empty() {
            let mut ids: Vec<_> = self.series_by_id.keys().copied().collect();
            ids.sort_unstable();
            return Ok(ids);
        }

        let mut postings = vec![];
        for matcher in matchers.iter() {
            let posting = self.query_postings(matcher)?;
            postings.push(posting);
        }

        let Some(smallets) = postings.iter().min_by_key(|posting| posting.len()) else {
            return Ok(Vec::new());
        };

        if smallets.is_empty() {
            return Ok(Vec::new());
        }

        let res = smallets
            .iter()
            .copied()
            .filter(|series_id| {
                postings
                    .iter()
                    .all(|posting| posting.binary_search(series_id).is_ok())
            })
            .collect();
        Ok(res)
    }
}

#[derive(Debug)]
pub struct QuerySeries {
    pub id: SeriesId,
    pub labels: Vec<Label>,
    pub samples: Vec<Sample>,
}

impl Head {
    pub fn new(shard_count: usize) -> Self {
        assert!(shard_count > 0, "shard count must be greater than zero");

        let shards = (0..shard_count)
            .map(|_| RwLock::new(Shard::default()))
            .collect();

        Self {
            shards,
            next_series_id: AtomicU64::new(0),
        }
    }

    fn shard_index(&self, key: &SeriesKey) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);

        hasher.finish() as usize % self.shards.len()
    }

    pub fn append(&self, key: SeriesKey, sample: Sample) -> Result<SeriesId, String> {
        let shard_index = self.shard_index(&key);
        let mut shard = self.shards[shard_index].write().unwrap();

        let series_id = match shard.series_by_key.get(&key).copied() {
            Some(id) => id,
            None => {
                let id = SeriesId(
                    self.next_series_id
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                );
                shard.create_series(id, key.clone());
                id
            }
        };

        shard
            .series_by_id
            .get_mut(&series_id)
            .expect("series indexes are inconsistent")
            .append(sample)?;
        Ok(series_id)
    }

    pub fn query_range(
        &self,
        matchers: &[LabelMatcher],
        range: TimeRange,
    ) -> Result<Vec<QuerySeries>, String> {
        let mut result = Vec::new();

        for shard in &self.shards {
            let shard = shard.read().unwrap();
            let series_ids = shard.select(matchers)?;
            for series_id in series_ids {
                let series = shard
                    .series_by_id
                    .get(&series_id)
                    .expect("series indexes are inconsistent");

                let samples = series.query_samples(&range).to_vec();
                if !samples.is_empty() {
                    result.push(QuerySeries {
                        id: series.id,
                        labels: series.key.labels.clone(),
                        samples: samples,
                    });
                }
            }
        }

        result.sort_unstable_by_key(|series| series.id);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        ops::Add,
        sync::Arc,
        thread,
        time::Duration,
    };

    use super::*;

    #[test]
    fn head_basic_op() {
        let h = Arc::new(Head::new(8));
        let t1 = {
            let h = Arc::clone(&h);

            thread::spawn(move || {
                loop {
                    let matchers = vec![LabelMatcher::new(
                        LabelMatcherOperator::Equal,
                        "__name__".to_string(),
                        "dummy_counter".to_string(),
                    )];
                    let range = TimeRange {
                        start: TimestampSecond(0),
                        end: TimestampSecond(200),
                    };

                    let res = h.query_range(&matchers, range).unwrap();
                    if res.is_empty() {
                        thread::sleep(Duration::from_millis(100));
                    } else {
                        break;
                    }
                }
            })
        };

        {
            thread::sleep(Duration::from_millis(300));
            let labels = vec![
                Label {
                    name: format!("__name__"),
                    value: format!("dummy_counter"),
                },
                Label {
                    name: format!("service"),
                    value: format!("foo"),
                },
            ];

            let series_key = SeriesKey::from(labels).unwrap();
            let sample = Sample {
                timestamp: TimestampSecond(123),
                value: f64::from(456),
            };

            h.append(series_key, sample).unwrap();

            let matchers = vec![LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "dummy_counter".to_string(),
            )];
            let range = TimeRange {
                start: TimestampSecond(0),
                end: TimestampSecond(200),
            };

            let res = h.query_range(&matchers, range).unwrap();

            assert_eq!(res.len(), 1);
            let samples = &res.first().unwrap().samples;
            assert_eq!(samples.len(), 1);
            let expected_sample = Sample {
                timestamp: TimestampSecond(123),
                value: f64::from(456),
            };
            assert_eq!(samples.first().unwrap().clone(), expected_sample);
        }
        t1.join().unwrap();
    }

    #[test]
    fn head_regex_match() {
        let h = Arc::new(Head::new(8));
        vec!["FooWriter", "BarWriter", "FooReader", "Sharder"]
            .into_iter()
            .for_each(|service| {
                let labels = vec![
                    Label {
                        name: format!("__name__"),
                        value: format!("dummy_counter"),
                    },
                    Label {
                        name: format!("service"),
                        value: service.to_string(),
                    },
                ];

                let series_key = SeriesKey::from(labels).unwrap();
                let sample = Sample {
                    timestamp: TimestampSecond(123),
                    value: f64::from(456),
                };

                h.append(series_key, sample).unwrap();
            });

        let matchers = vec![
            LabelMatcher::new(
                LabelMatcherOperator::Equal,
                "__name__".to_string(),
                "dummy_counter".to_string(),
            ),
            LabelMatcher::new(
                LabelMatcherOperator::RegexMatch,
                "service".to_string(),
                ".*Writer".to_string(),
            ),
        ];
        let range = TimeRange {
            start: TimestampSecond(0),
            end: TimestampSecond(200),
        };

        let res = h.query_range(&matchers, range).unwrap();

        assert_eq!(res.len(), 2);
    }

    #[test]
    fn test_threads() {
        let a = Cell::new(5);
        let b = RefCell::new(5);

        {
            a.set(6);
        }
        {
            b.borrow_mut().add(3);
        }
    }
}
