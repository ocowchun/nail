use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use nail::api::{APIContext, handle_request};
use nail::config::read_config;
use nail::head::Head;
use nail::scraper::Scraper;
use tokio::net::TcpListener;
use tokio::time;
use tokio_metrics::RuntimeMetricsReporterBuilder;

async fn log(message: &str) {
    let now = Local::now();
    println!("{} {message}", now.format("%Y-%m-%d %H:%M:%S"))
}

async fn scrape(scraper: Scraper) {
    println!("schedule scrape job");
    // TODO: handle interval by job individually
    let mut interval = time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        log("start scrape").await;
        if let Err(err) = scraper.scrape().await {
            log(&format!("scrape failed, {}", err)).await;
        } else {
            log("complete scrape").await;
        }
    }
}

fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration_seconds".to_owned()),
            &[0.05, 0.1, 0.2, 0.5, 1.0, 2.5, 5.0, 10.0, 20.0, 30.0],
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Full("scrape_duration_seconds".to_owned()),
            &[0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.5, 5.0, 10.0],
        )
        .unwrap()
        .install_recorder()
        .expect("failed to install metrics recorder")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = read_config("prometheus.example.yml").await?;

    println!("config -> {:?}", config);
    println!("global {:?}", config.global.scrape_interval);

    let address = SocketAddr::from(([127, 0, 0, 1], 9091));
    let listener = TcpListener::bind(address).await?;

    let head = Arc::new(Head::new(1));
    let metric_handle = Arc::new(init_metrics());

    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(5))
            .describe_and_run(),
    );

    {
        let scraper = Scraper::new(
            Arc::clone(&head),
            Arc::clone(&metric_handle),
            config.scrape_configs,
        );
        tokio::spawn(async move {
            scrape(scraper).await;
        });
    }

    println!("starting http connection");

    loop {
        let (stream, _) = listener.accept().await?;

        let connection_head = Arc::clone(&head);
        let connection_metric_handle = Arc::clone(&metric_handle);
        tokio::spawn(async move {
            let io = TokioIo::new(stream);

            let service = service_fn(move |request| {
                let head = Arc::clone(&connection_head);
                let metric_handle = Arc::clone(&connection_metric_handle);
                let api_context = Arc::new(APIContext::new(head, metric_handle));

                async move { handle_request(request, api_context).await }
            });

            if let Err(error) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("connection error: {error}")
            }
        });
    }
}
