use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::Context;
use std::time::{Duration, Instant};

use bytes::Bytes;
use chrono::Local;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::CONTENT_TYPE;
use hyper::{Method, Request, Response, StatusCode};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::{Deserialize, Serialize};

use crate::core::Timestamp;
use crate::head::Head;
use crate::query_exec::QueryExec;
use crate::request::{QueryLabelValuesRequest, QueryRangeRequest, QueryRequest};

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

#[derive(Debug, Serialize)]
struct RangeSeries {
    metric: HashMap<String, String>,
    values: Vec<(f64, String)>,
}

#[derive(Debug, Serialize)]
struct InstantSeries {
    metric: HashMap<String, String>,
    value: (f64, String),
}

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    status: &'static str,
    error_type: String,
    error: String,
}

fn error_response(status: StatusCode, error_type: String, error: String) -> Response<ResponseBody> {
    let err = ErrorResponse {
        status: "error",
        error_type: error_type,
        error: error,
    };

    match serde_json::to_vec(&err) {
        Ok(json) => Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(json)))
            .unwrap(),
        Err(error) => {
            eprintln!("failed to seralize response: {error}");
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n")
        }
    }
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

async fn handle_query_range(
    request: Request<Incoming>,
    context: Arc<APIContext>,
) -> Response<ResponseBody> {
    let req = match parse_query_range_request(request).await {
        Ok(req) => req,
        Err(_) => {
            // TODO: handle error properly
            return text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n");
        }
    };

    let query_exec = QueryExec::new(context.head.clone());
    let res = match query_exec.query_range(req) {
        Ok(res) => res,
        Err(err) => match err {
            crate::query_exec::QueryError::InvalidRequest(err) => {
                return error_response(StatusCode::BAD_REQUEST, "bad_data".to_owned(), err);
            }
            crate::query_exec::QueryError::InternalError(err) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "interna_server_error".to_owned(),
                    err,
                );
            }
        },
    };

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

    QueryRequest::try_from(raw_request)
}

async fn handle_query(
    request: Request<Incoming>,
    context: Arc<APIContext>,
) -> Response<ResponseBody> {
    let req = match parse_query_request(request).await {
        Ok(req) => req,
        Err(_) => {
            // TODO: handle error properly
            return text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error\n");
        }
    };

    let query_exec = QueryExec::new(Arc::clone(&context.head));
    let res = match query_exec.query(req) {
        Ok(res) => res,
        Err(err) => match err {
            crate::query_exec::QueryError::InvalidRequest(err) => {
                return error_response(StatusCode::BAD_REQUEST, "bad_data".to_owned(), err);
            }
            crate::query_exec::QueryError::InternalError(err) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "interna_server_error".to_owned(),
                    err,
                );
            }
        },
    };

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

async fn handle_label_values(
    _request: Request<Incoming>,
    label_name: &str,
    context: Arc<APIContext>,
) -> Response<ResponseBody> {
    let req = QueryLabelValuesRequest {
        label_name: label_name.to_string(),
    };
    let label_values: Vec<_> = context.head.query_label_values(req).into_iter().collect();
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

async fn handle_dynamic_path(
    request: Request<Incoming>,
    context: Arc<APIContext>,
) -> Response<ResponseBody> {
    let path = request.uri().path().to_owned();
    if request.method() == &Method::GET
        && path.starts_with("/api/v1/label/")
        && path.ends_with("/values")
    {
        let label_name = path
            .strip_prefix("/api/v1/label/")
            .unwrap()
            .strip_suffix("/values")
            .unwrap();
        return handle_label_values(request, label_name, context).await;
    } else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from_static(b"not found\n")))
            .unwrap();
    }
}

async fn handle_metrics(context: Arc<APIContext>) -> Response<ResponseBody> {
    Response::builder()
        .header("content-type", "text/plain; version=0.0.4 charset=utf-8")
        .body(Full::new(Bytes::from(context.metric_handle.render())))
        .unwrap()
}

pub struct APIContext {
    head: Arc<Head>,
    metric_handle: Arc<PrometheusHandle>,
}

impl APIContext {
    pub fn new(head: Arc<Head>, metric_handle: Arc<PrometheusHandle>) -> Self {
        Self {
            head,
            metric_handle,
        }
    }
}

// 2026-08-22 19:02:40 GET /api/v1/labels 404
// 2026-08-22 19:02:40 GET /api/v1/label/__name__/values 404
// 2026-08-22 19:02:40 GET /api/v1/metadata 404
// ^A32026-08-22 19:49:31 GET /api/v1/rules 404
// 2026-08-22 19:49:31 GET /api/v1/query_exemplars 404
pub async fn handle_request(
    request: Request<Incoming>,
    context: Arc<APIContext>, // head: Arc<Head>,
) -> Result<Response<ResponseBody>, Infallible> {
    let started = Instant::now();

    let method = request.method().to_string();
    let path = request.uri().path_and_query().unwrap().as_str().to_owned();

    let route = if path.starts_with("/api/v1/label/") && path.ends_with("/values") {
        "/api/v1/label/:name/values".to_owned()
    } else {
        path.clone()
    };

    let response = match (request.method(), request.uri().path()) {
        (_, "/-/healthy") => Response::new(Full::new(Bytes::from_static(b"healthy\n"))),
        (&Method::GET, "/api/v1/query_range") => handle_query_range(request, context).await,
        (&Method::POST, "/api/v1/query_range") => handle_query_range(request, context).await,
        (&Method::GET, "/api/v1/status/buildinfo") => handle_build_info().await,
        (&Method::POST, "/api/v1/query") => handle_query(request, context).await,
        (&Method::GET, "/api/v1/rules") => handle_rules().await,
        (&Method::GET, "/metrics") => handle_metrics(context).await,

        _ => handle_dynamic_path(request, context).await,
    };
    let message = format!("{} {} {}", method, path, response.status().as_str());
    log(&message).await;

    let elapsed = started.elapsed();
    metrics::histogram!(
        "http_request_duration_seconds",
            "method" => method,
            "route" => route,
            "status" => response.status().as_u16().to_string()
    )
    .record(elapsed.as_secs_f64());

    Ok(response)
}
