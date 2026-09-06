use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::Local;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use nail::config::{Config, read_config};
use nail::core::{Label, Labels, Timestamp};
use nail::head::Head;
use nail::query_exec::QueryExec;
use nail::request::{QueryRangeRequest, QueryRequest};
use nail::scraper::{ScrapeConfig, Scraper, StaticConfig};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::{fs, time};

type ResponseBody = Full<Bytes>;

async fn log(message: &str) {
    let now = Local::now();
    println!("{} {message}", now.format("%Y-%m-%d %H:%M:%S"))
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    status: &'static str,
    data: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRangeData {
    result_type: &'static str,
    result: Vec<RangeSeries>,
}

type RangeVectors = Vec<RangeSeries>;

#[derive(Debug, Serialize)]
struct RangeSeries {
    metric: HashMap<String, String>,
    values: Vec<(f64, String)>,
}

type InstantVectors = Vec<InstantSeries>;

#[derive(Debug, Serialize)]
struct InstantSeries {
    metric: HashMap<String, String>,
    value: (f64, String),
}

type Scalar = (f64, String);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildInfo {
    version: &'static str,
    revision: &'static str,
    branch: &'static str,
    build_user: &'static str,
    build_date: &'static str,
    go_version: &'static str,
}

fn text_response(status: StatusCode, body: &str) -> Response<ResponseBody> {
    let body = Full::new(Bytes::from(body.to_string()));
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body)
        .unwrap()
}

async fn handle_build_info() -> Response<ResponseBody> {
    let build_info = BuildInfo {
        version: "3.14.0",
        revision: "d7598b7141418fa35be2b5ec5d0fefb634199610",
        branch: "HEAD",
        build_user: "root@4c568bad4aae",
        build_date: "20260817-16:46:08",
        go_version: "go1.26.6",
    };

    match serde_json::to_vec(&build_info) {
        Ok(json) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(json)))
            .unwrap(),
        Err(error) => {
            eprintln!("failed to seralize response: {error}");
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n")
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RawQueryRangeRequest {
    query: String,
    start: String,
    end: String,
    step: String,
}

fn parse_step(input: &str) -> Result<Duration, String> {
    let duration = if let Ok(seconds) = input.parse::<f64>() {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(format!("step must be a positive finite number"));
        }
        Duration::from_secs_f64(seconds)
    } else {
        humantime::parse_duration(input).map_err(|_| format!("invalid step: {input}"))?
    };

    if duration.is_zero() {
        return Err(format!("step must be greater than zero"));
    }

    Ok(duration)
}

impl TryFrom<RawQueryRangeRequest> for QueryRangeRequest {
    type Error = String;

    fn try_from(raw: RawQueryRangeRequest) -> Result<Self, Self::Error> {
        if raw.query.trim().is_empty() {
            return Err(format!("query must not be empty"));
        }

        let start = Timestamp::parse(&raw.start)?;
        let end = Timestamp::parse(&raw.end)?;
        if end < start {
            return Err(format!(
                "end timestamp must not be earlier than start timestamp"
            ));
        }
        let step = parse_step(&raw.step)?;

        Ok(Self {
            query: raw.query,
            start,
            end,
            step,
        })
    }
}

async fn parse_query_range_request(
    request: Request<Incoming>,
) -> Result<QueryRangeRequest, String> {
    let bytes = match request.into_body().collect().await {
        Ok(body) => body.to_bytes(),
        Err(error) => {
            return Err(error.to_string());
        }
    };

    let raw_request = match serde_urlencoded::from_bytes::<RawQueryRangeRequest>(&bytes) {
        Ok(value) => value,
        Err(error) => {
            return Err(error.to_string());
        }
    };

    QueryRangeRequest::try_from(raw_request)
}

async fn handle_query_range(request: Request<Incoming>, head: Arc<Head>) -> Response<ResponseBody> {
    let req = match parse_query_range_request(request).await {
        Ok(req) => req,
        Err(_) => {
            // TODO: handle error properly
            return text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n");
        }
    };

    let query_exec = QueryExec::new(head.clone());
    let res = match query_exec.query_range(req) {
        Ok(res) => res,
        Err(err) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("internal server error {err}\n"),
            );
        }
    };

    // let metric = BTreeMap::from([
    //     ("__name__".to_owned(), "dummy_cpu_usage".to_owned()),
    //     ("instance".to_owned(), "localhost:9090".to_owned()),
    //     ("job".to_owned(), "dummy".to_owned()),
    // ]);

    // let start = req.start.as_seconds();
    // let end = req.end.as_seconds();
    // let step = req.step.as_secs_f64();
    // let sample_count = ((end - start) / step).floor() as usize + 1;

    // let values: Vec<(f64, String)> = (0..sample_count)
    //     .map(|index| {
    //         let timestamp = start + index as f64 * step;
    //         let value = 50.0 + (index as f64 / 5.0).sin() * 10.0;

    //         (timestamp, format!("{value:.2}"))
    //     })
    //     .collect();

    // #[derive(Debug, Serialize)]
    // struct RangeSeries {
    //     metric: BTreeMap<String, String>,
    //     values: Vec<(f64, String)>,
    // }
    let result = res
        .result
        .into_iter()
        .map(|series| {
            let metric: HashMap<_, _> = series
                .labels
                .into_iter()
                .map(|label| (label.name, label.value))
                .collect();
            let values = series
                .samples
                .into_iter()
                .map(|sample| {
                    let timestamp = sample.timestamp.0 as f64;
                    let value = sample.value;
                    let value = format!("{value:.2}");
                    (timestamp, value)
                })
                .collect();
            RangeSeries { metric, values }
        })
        .collect();

    // let series = RangeSeries {
    //     metric,
    //     values: values, // values: vec![
    //                     //     (1_787_300_000.0, "0.42".to_owned()),
    //                     //     (1_787_300_015.0, "0.46".to_owned()),
    //                     //     (1_787_300_030.0, "0.51".to_owned()),
    //                     //     (1_787_300_045.0, "0.48".to_owned()),
    //                     // ],
    // };

    let response = ApiResponse {
        status: "success",
        data: QueryRangeData {
            result_type: "matrix",
            result,
        },
    };

    match serde_json::to_vec(&response) {
        Ok(json) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(json)))
            .unwrap(),
        Err(error) => {
            eprintln!("failed to seralize response: {error}");
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n")
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryApiResponseData<T> {
    result_type: &'static str,
    result: T,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawQueryRequest {
    query: String,
    time: Option<String>,
    timeout: Option<String>,
    // https://prometheus.io/docs/prometheus/latest/querying/api/#instant-queries
}

impl TryFrom<RawQueryRequest> for QueryRequest {
    type Error = String;

    fn try_from(raw: RawQueryRequest) -> Result<Self, Self::Error> {
        if raw.query.trim().is_empty() {
            return Err(format!("query must not be empty"));
        }

        let time = if let Some(time) = raw.time {
            Timestamp::parse(&time)?
        } else {
            Timestamp::now()
        };

        let timeout = Duration::from_secs(10);

        Ok(Self {
            query: raw.query,
            time,
            timeout,
        })
    }
}

async fn parse_query_request(request: Request<Incoming>) -> Result<QueryRequest, String> {
    let bytes = match request.into_body().collect().await {
        Ok(body) => body.to_bytes(),
        Err(error) => {
            return Err(error.to_string());
        }
    };

    let raw_request = match serde_urlencoded::from_bytes::<RawQueryRequest>(&bytes) {
        Ok(value) => value,
        Err(error) => {
            return Err(error.to_string());
        }
    };

    println!("query_request-> {:?}", raw_request);

    QueryRequest::try_from(raw_request)
}

async fn handle_query(request: Request<Incoming>, head: Arc<Head>) -> Response<ResponseBody> {
    let req = match parse_query_request(request).await {
        Ok(req) => req,
        Err(_) => {
            // TODO: handle error properly
            return text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n");
        }
    };

    let query_exec = QueryExec::new(head.clone());
    println!("query req -> {:?}", &req);
    let res = match query_exec.query(req) {
        Ok(res) => res,
        Err(err) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("internal server error {err}\n"),
            );
        }
    };
    // TODO
    // text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    // let scalar = (1_787_300_045.0, "0.48".to_owned());

    let result: Vec<InstantSeries> = res
        .result
        .into_iter()
        .map(|sample| {
            let metric: HashMap<_, _> = sample
                .labels
                .into_iter()
                .map(|label| (label.name, label.value))
                .collect();

            let timestamp = sample.sample.timestamp.0 as f64;
            let value = sample.sample.value;
            let value = format!("{value:.2}");
            let value = (timestamp, value);
            InstantSeries { metric, value }
        })
        .collect();

    let response = ApiResponse {
        status: "success",
        data: QueryApiResponseData {
            result_type: "vector",
            result,
        },
    };

    match serde_json::to_vec(&response) {
        Ok(json) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(json)))
            .unwrap(),
        Err(error) => {
            eprintln!("failed to seralize response: {error}");
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n")
        }
    }
}

async fn handle_metadata(request: Request<Incoming>) -> Response<ResponseBody> {
    todo!()
}

async fn handle_rules() -> Response<ResponseBody> {
    // TODO
    text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
}

async fn handle_label_values(request: Request<Incoming>) -> Response<ResponseBody> {
    let label_values = vec!["horenso", "cyasyou", "tamago", "nori"];
    let response = ApiResponse {
        status: "success",
        data: label_values,
    };

    match serde_json::to_vec(&response) {
        Ok(json) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(json)))
            .unwrap(),
        Err(error) => {
            eprintln!("failed to seralize response: {error}");
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n")
        }
    }
}

async fn handle_dynamic_path(request: Request<Incoming>) -> Response<ResponseBody> {
    let path = request.uri().path().to_owned();
    if request.method() == &Method::GET
        && path.starts_with("/api/v1/label/")
        && path.ends_with("/values")
    {
        return handle_label_values(request).await;
    } else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from_static(b"not found\n")))
            .unwrap();
    }
}

// 2026-08-22 19:02:40 GET /api/v1/labels 404
// 2026-08-22 19:02:40 GET /api/v1/label/__name__/values 404
// 2026-08-22 19:02:40 GET /api/v1/metadata 404
// ^A32026-08-22 19:49:31 GET /api/v1/rules 404
// 2026-08-22 19:49:31 GET /api/v1/query_exemplars 404
async fn handle_request(
    request: Request<Incoming>,
    head: Arc<Head>,
) -> Result<Response<ResponseBody>, Infallible> {
    let method = request.method().to_string();
    let path = request.uri().path_and_query().unwrap().as_str().to_owned();
    let response = match (request.method(), request.uri().path()) {
        (_, "/-/healthy") => Response::new(Full::new(Bytes::from_static(b"healthy\n"))),
        (&Method::GET, "/api/v1/query_range") => handle_query_range(request, head).await,
        (&Method::POST, "/api/v1/query_range") => handle_query_range(request, head).await,
        (&Method::GET, "/api/v1/status/buildinfo") => handle_build_info().await,
        (&Method::POST, "/api/v1/query") => handle_query(request, head).await,
        (&Method::GET, "/api/v1/rules") => handle_rules().await,

        _ => handle_dynamic_path(request).await,
    };
    let message = format!("{} {} {}", method, path, response.status().as_str());
    log(&message).await;

    Ok(response)
}

fn demo() {
    let metric = HashMap::from([
        ("__name__".to_owned(), "dummy_cpu_usage".to_owned()),
        ("instance".to_owned(), "localhost:9090".to_owned()),
        ("job".to_owned(), "dummy".to_owned()),
    ]);

    // let series = RangeSeries {
    //     metric,
    //     values: vec![
    //         (1_787_300_000.0, "0.42".to_owned()),
    //         (1_787_300_015.0, "0.46".to_owned()),
    //         (1_787_300_030.0, "0.51".to_owned()),
    //         (1_787_300_045.0, "0.48".to_owned()),
    //     ],
    // };

    // let response = ApiResponse {
    //     status: "success",
    //     data: QueryRangeData {
    //         result_type: "matrix",
    //         result: vec![series],
    //     },
    // };
    let instant_series = InstantSeries {
        metric,
        value: (1_787_300_045.0, "0.48".to_owned()),
    };

    let instant_vector = vec![instant_series];
    let scalar = (1_787_300_045.0, "0.48".to_owned());

    let response = ApiResponse {
        status: "success",
        data: QueryApiResponseData {
            result_type: "scalar",
            result: scalar,
        },
    };

    match serde_json::to_string(&response) {
        Ok(json) => println!("yo -> {}", json),
        Err(error) => {
            eprintln!("failed to seralize response: {error}");
        }
    };
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
            // println!("run scrape");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Hello, world!");

    demo();

    println!("===");

    let config = read_config("prometheus.example.yml").await?;

    println!("config -> {:?}", config);
    println!("global {:?}", config.global.scrape_interval);

    let address = SocketAddr::from(([127, 0, 0, 1], 9091));
    let listener = TcpListener::bind(address).await?;

    let head = Arc::new(Head::new(1));
    {
        let scraper = Scraper::new(Arc::clone(&head), config.scrape_configs);
        tokio::spawn(async move {
            scrape(scraper).await;
        });
    }

    println!("starting http connection");

    loop {
        let (stream, _) = listener.accept().await?;

        let connection_head = Arc::clone(&head);
        tokio::spawn(async move {
            let io = TokioIo::new(stream);

            let service = service_fn(move |request| {
                let head = Arc::clone(&connection_head);
                async move { handle_request(request, head).await }
            });

            if let Err(error) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("connection error: {error}")
            }
        });
    }
}
