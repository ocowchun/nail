use std::{iter::Peekable, str::Chars, sync::Arc, time::Duration};

use chrono::Local;
use futures::{StreamExt, stream};
use itertools::{Either, Itertools};
use reqwest::Client;

use crate::{
    core::{Label, Labels},
    head::{Head, Sample, SeriesKey, TimestampSecond},
};

#[derive(Debug)]
pub struct ScrapeConfig {
    pub job_name: String,
    pub scrape_interval: Duration,
    pub scrape_timeout: Duration,
    pub static_configs: Vec<StaticConfig>,
}
impl ScrapeConfig {
    pub fn new(
        job_name: String,
        scrape_interval: Duration,
        scrape_timeout: Duration,
        static_configs: Vec<StaticConfig>,
    ) -> Self {
        Self {
            job_name,
            scrape_interval,
            scrape_timeout,
            static_configs,
        }
    }
}

#[derive(Debug)]
pub struct StaticConfig {
    pub targets: Vec<String>,
    pub labels: Labels,
}

impl StaticConfig {
    pub fn new(targets: Vec<String>, labels: Labels) -> Self {
        Self { targets, labels }
    }
}

struct ScrapeTask {
    default_labels: Labels,
    target_url: String,
    scrape_timeout: Duration,
}

pub struct Scraper {
    head: Arc<Head>,
    configs: Vec<ScrapeConfig>,
}

impl Scraper {
    pub fn new(head: Arc<Head>, configs: Vec<ScrapeConfig>) -> Self {
        Self { head, configs }
    }

    pub async fn scrape(&self) -> Result<(), String> {
        let now = Local::now();
        let timestamp = TimestampSecond(now.timestamp());
        println!("run scrape, timestamp -> {}", timestamp.0);

        let mut tasks = vec![];
        for config in self.configs.iter() {
            let job_labels =
                Labels::from(vec![Label::new("job".to_string(), config.job_name.clone())]).unwrap();

            for static_config in config.static_configs.iter() {
                let default_labels = match static_config.labels.merge(&job_labels) {
                    Ok(labels) => labels,
                    Err(err) => return Err(err),
                };
                for target in static_config.targets.iter() {
                    let task = ScrapeTask {
                        default_labels: default_labels.clone(),
                        target_url: target.clone(),
                        scrape_timeout: config.scrape_timeout,
                    };
                    tasks.push(task);
                }
            }
        }

        let max_concurrency = 3;
        let fetched_results = stream::iter(tasks)
            .map(|task| Self::fetch(task))
            .buffer_unordered(max_concurrency)
            .collect::<Vec<_>>()
            .await;

        let mut appended = 0;
        for result in fetched_results.into_iter() {
            match result {
                Ok(metrics) => {
                    for metric in metrics.into_iter() {
                        let key = SeriesKey::from(metric.labels.labels().into()).unwrap();
                        let sample = Sample::new(timestamp, metric.point.value);

                        if let Err(err) = self.head.append(key, sample) {
                            println!("append failed, {}", err);
                        } else {
                            appended += 1;
                        }
                    }
                }
                Err(err) => {
                    println!("fetch failed, {}", err);
                }
            }
        }

        println!("append {} samples", appended);

        Ok(())
    }

    async fn fetch(task: ScrapeTask) -> Result<Vec<Metric>, String> {
        let client = Client::new();
        let res = client
            .get(task.target_url)
            .timeout(task.scrape_timeout)
            .send();
        let res = match res.await {
            Ok(res) => res,
            Err(err) => {
                return Err(err.to_string());
            }
        };

        if !res.status().is_success() {
            return Err(format!("received status code: {}", res.status().as_str()));
        }

        let text = res.text().await.unwrap();
        let metrics = MetricsParser::parse(&text)?;

        // for metric in metrics.iter() {

        // }
        let (metrics, errors): (Vec<_>, Vec<_>) = metrics
            .into_iter()
            .map(|metric| match metric.labels.merge(&task.default_labels) {
                Ok(labels) => Ok(Metric {
                    labels,
                    point: metric.point,
                }),
                Err(err) => Err(err),
            })
            .partition_map(|result| match result {
                Ok(value) => Either::Left(value),
                Err(error) => Either::Right(error),
            });

        for error in errors {
            println!("{}", error);
        }

        Ok(metrics)
    }
}

#[derive(Debug, PartialEq)]
struct MetricPoint {
    // TODO: should we put timestamp?
    value: f64,
}

#[derive(Debug, PartialEq)]
struct Metric {
    pub labels: Labels,
    pub point: MetricPoint,
}

// TODO: should we handle type?
struct MetricsParser {}

impl MetricsParser {
    pub fn parse(text: &str) -> Result<Vec<Metric>, String> {
        let mut metrics = vec![];
        for line in text.lines() {
            if line.starts_with("#") || line.is_empty() {
                // should we handle HELP and TYPE?
                continue;
            }

            let metric = Self::parse_metric(line)?;
            metrics.push(metric);
        }
        Ok(metrics)
    }

    fn parse_metric(line: &str) -> Result<Metric, String> {
        // prometheus_sd_kubernetes_events_total{event="add",role="endpoints"} 0
        let mut iter = line.chars();
        let mut name = String::new();
        let mut has_label = true;
        loop {
            let c = match iter.next() {
                Some(c) => c,
                None => {
                    return Err(format!("can't parse name"));
                }
            };
            if c == ' ' {
                has_label = false;
                break;
            } else if c == '{' {
                break;
            }
            name.push(c);
        }

        let mut labels = vec![Label::new("__name__".to_string(), name)];
        if has_label {
            loop {
                let label_name = match Self::next_until(&mut iter, '=') {
                    Ok(str) => str,
                    Err(err) => {
                        return Err(format!("can't parse label_name, err: {}", err));
                    }
                };

                if let Some(c) = iter.next()
                    && c == '"'
                {
                } else {
                    return Err(format!("can't parse value {}", line));
                }

                let label_value = match Self::next_until(&mut iter, '"') {
                    Ok(str) => str,
                    Err(err) => {
                        return Err(format!("can't parse label_value1, err: {}", err));
                    }
                };
                labels.push(Label::new(label_name, label_value));

                if let Some(c) = iter.next() {
                    if c == ',' {
                        continue;
                    } else if c == '}' {
                        break;
                    } else {
                        return Err(format!("can't parse value2.1 `{}` {}", c, line));
                    }
                } else {
                    return Err(format!("can't parse value2.2 {}", line));
                }
            }

            if let Some(c) = iter.next()
                && c == ' '
            {
            } else {
                return Err(format!("can't parse value3 {}", line));
            }
        }

        let mut a = String::new();
        while let Some(c) = iter.next() {
            a.push(c);
        }

        let val = match a.parse::<f64>() {
            Ok(val) => val,
            Err(err) => {
                return Err(format!("can't parse value, err: {}", err));
            }
        };

        let labels = match Labels::from(labels) {
            Ok(labels) => labels,
            Err(err) => {
                return Err(format!("can't parse metric, err: {}", err));
            }
        };

        let metric = Metric {
            labels,
            point: MetricPoint { value: val },
        };
        Ok(metric)
    }

    fn next_until(iter: &mut Chars<'_>, end: char) -> Result<String, String> {
        let mut res = String::new();
        while let Some(c) = iter.next() {
            if c == end {
                return Ok(res);
            }

            res.push(c);
        }
        Err(format!("can't find end: `{}`", end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metric() {
        let mut text = vec![
            "# TYPE go_gc_cleanups_executed_cleanups_total counter",
            "go_gc_cleanups_executed_cleanups_total 0",
            "# TYPE prometheus_sd_kubernetes_events_total counter",
            "prometheus_sd_kubernetes_events_total{event=\"add\",role=\"endpoints\"} 160398",
            "prometheus_sd_kubernetes_events_total{event=\"add\",role=\"endpointslice\"} 2.577248e+06",
        ]
        .join("\n");

        let res = MetricsParser::parse(&text).unwrap();

        assert_eq!(res.len(), 3);
        let expected_metrics = vec![
            Metric {
                labels: Labels::from(vec![Label::new(
                    "__name__".to_string(),
                    "go_gc_cleanups_executed_cleanups_total".to_string(),
                )])
                .unwrap(),
                point: MetricPoint {
                    value: "0".parse::<f64>().unwrap(),
                },
            },
            Metric {
                labels: Labels::from(vec![
                    Label::new(
                        "__name__".to_string(),
                        "prometheus_sd_kubernetes_events_total".to_string(),
                    ),
                    Label::new("event".to_string(), "add".to_string()),
                    Label::new("role".to_string(), "endpoints".to_string()),
                ])
                .unwrap(),
                point: MetricPoint {
                    value: "160398".parse::<f64>().unwrap(),
                },
            },
            Metric {
                labels: Labels::from(vec![
                    Label::new(
                        "__name__".to_string(),
                        "prometheus_sd_kubernetes_events_total".to_string(),
                    ),
                    Label::new("event".to_string(), "add".to_string()),
                    Label::new("role".to_string(), "endpointslice".to_string()),
                ])
                .unwrap(),
                point: MetricPoint {
                    value: "2.577248e+06".parse::<f64>().unwrap(),
                },
            },
        ];
        for (index, expected_metric) in expected_metrics.iter().enumerate() {
            let metric = &res[index];
            assert_eq!(metric, expected_metric);
        }
    }
}
