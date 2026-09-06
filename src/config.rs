use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tokio::fs;

use crate::{
    core::{Label, Labels},
    scraper::{ScrapeConfig, StaticConfig},
};

#[derive(Debug, Deserialize)]
pub struct GlobalConfig {
    pub scrape_interval: Duration,
    pub evaluation_interval: Duration,
    pub scrape_timeout: Duration,
}

#[derive(Debug)]
pub struct Config {
    pub global: GlobalConfig,
    pub scrape_configs: Vec<ScrapeConfig>,
}

#[derive(Debug, Deserialize)]
struct RawGlobalConfig {
    scrape_interval: Option<String>,
    evaluation_interval: String,
    scrape_timeout: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawStaticScrapeConfig {
    targets: Vec<String>,
    labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct RawScrapeConfig {
    job_name: String,
    static_configs: Vec<RawStaticScrapeConfig>,
    scrape_native_histograms: bool,
    scrape_interval: Option<String>,
    scrape_timeout: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    global: RawGlobalConfig,
    scrape_configs: Vec<RawScrapeConfig>,
}

pub async fn read_config(config_path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(config_path).await?;
    let raw_config: RawConfig = serde_yaml_ng::from_str(&content)?;
    println!("raw_config -> {:?}", raw_config);

    let global_config = parse_global_config(&raw_config.global)?;
    let scrape_configs = parse_scrape_configs(&raw_config.scrape_configs, &global_config)?;

    Ok(Config {
        global: global_config,
        scrape_configs,
    })
}

fn parse_global_config(
    raw_global_config: &RawGlobalConfig,
) -> Result<GlobalConfig, Box<dyn std::error::Error>> {
    let scrape_interval = if let Some(scrape_interval) = &raw_global_config.scrape_interval {
        match parse_duration(scrape_interval) {
            Ok(interval) => interval,
            Err(error) => return Err(error.into()),
        }
    } else {
        Duration::from_secs(60)
    };

    let scrape_timeout = if let Some(scrape_timeout) = &raw_global_config.scrape_timeout {
        match parse_duration(scrape_timeout) {
            Ok(timeout) => timeout,
            Err(error) => return Err(error.into()),
        }
    } else {
        Duration::from_secs(10)
    };

    let evaluation_interval = match parse_duration(&raw_global_config.evaluation_interval) {
        Ok(interval) => interval,
        Err(error) => return Err(error.into()),
    };

    Ok(GlobalConfig {
        scrape_interval,
        evaluation_interval,
        scrape_timeout,
    })
}

fn parse_scrape_configs(
    raw_scrape_configs: &Vec<RawScrapeConfig>,
    global_config: &GlobalConfig,
) -> Result<Vec<ScrapeConfig>, Box<dyn std::error::Error>> {
    let mut scrape_configs = vec![];
    for raw_scrape_config in raw_scrape_configs.iter() {
        let job_name = raw_scrape_config.job_name.clone();
        let mut static_configs = vec![];
        for raw_static_config in raw_scrape_config.static_configs.iter() {
            let labels = raw_static_config
                .labels
                .iter()
                .map(|(key, val)| Label::new(key.to_string(), val.to_string()))
                .collect::<Vec<_>>();
            let labels = Labels::from(labels)?;

            // TODO: handle scheme and path
            let targets = raw_static_config
                .targets
                .iter()
                .map(|target| format!("http://{target}/metrics"))
                .collect();

            static_configs.push(StaticConfig::new(targets, labels));
        }

        let scrape_interval = if let Some(scrape_interval) = &raw_scrape_config.scrape_interval {
            match parse_duration(scrape_interval) {
                Ok(interval) => interval,
                Err(error) => return Err(error.into()),
            }
        } else {
            global_config.scrape_interval
        };
        let scrape_timeout = if let Some(scrape_timeout) = &raw_scrape_config.scrape_timeout {
            match parse_duration(scrape_timeout) {
                Ok(timeout) => timeout,
                Err(error) => return Err(error.into()),
            }
        } else {
            global_config.scrape_timeout
        };

        scrape_configs.push(ScrapeConfig::new(
            job_name,
            scrape_interval,
            scrape_timeout,
            static_configs,
        ));
    }

    Ok(scrape_configs)
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    if let Some(seconds) = value.strip_suffix('s') {
        let seconds = seconds
            .parse::<u64>()
            .map_err(|_| format!("invalid interval: {value}"))?;

        return Ok(Duration::from_secs(seconds));
    }

    Err(format!("invalid interval: {value}"))
}
