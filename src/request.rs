use std::time::Duration;

use crate::core::Timestamp;

pub struct QueryRangeRequest {
    pub query: String,
    pub start: Timestamp,
    pub end: Timestamp,
    pub step: Duration,
}

#[derive(Debug)]
pub struct QueryRequest {
    pub query: String,
    pub time: Timestamp,
    pub timeout: Duration,
    // https://prometheus.io/docs/prometheus/latest/querying/api/#instant-queries
}
