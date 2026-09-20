use chrono::{DateTime, Datelike, Timelike};

use crate::{
    analyzer::ExpressionType,
    core::{Sample, TimestampSecond},
    function::types::{EvalValue, FunctionSpec, QueryContext},
    query_exec::{InstantSeries, SeriesListInstantIterator},
};

fn base_eval(
    args: Vec<EvalValue>,
    context: QueryContext,
    transform: fn(i64) -> f64,
) -> Result<EvalValue, String> {
    if args.len() != 0 {
        unreachable!("validated by analyzer");
    }
    let samples = context
        .query_points
        .into_iter()
        .map(|v| {
            let val = transform(v);
            Sample::new(TimestampSecond(v), val)
        })
        .collect();

    let series = InstantSeries::new(vec![], samples);
    let iter = SeriesListInstantIterator::new(vec![series]);
    Ok(EvalValue::Instant(Box::new(iter)))
}

fn eval_time(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| v as f64)
}

pub static TIME_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "time",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_time,
};

fn eval_day_of_month(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v).unwrap().day() as f64
    })
}

pub static DAY_OF_MONTH_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "day_of_month",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_day_of_month,
};

fn eval_day_of_week(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v)
            .unwrap()
            .weekday()
            .num_days_from_sunday() as f64
    })
}

pub static DAY_OF_WEEK_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "day_of_week",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_day_of_week,
};

fn eval_day_of_year(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v).unwrap().ordinal() as f64
    })
}

pub static DAY_OF_YEAR_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "day_of_year",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_day_of_year,
};

fn eval_days_in_month(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v)
            .unwrap()
            .num_days_in_month() as f64
    })
}

pub static DAYS_IN_MONTH_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "days_in_month",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_days_in_month,
};

fn eval_hour(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v).unwrap().hour() as f64
    })
}

pub static HOUR_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "hour",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_hour,
};

fn eval_minute(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v).unwrap().minute() as f64
    })
}

pub static MINUTE_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "minute",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_minute,
};

fn eval_month(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v).unwrap().month() as f64
    })
}

pub static MONTH_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "month",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_month,
};

fn eval_year(args: Vec<EvalValue>, context: QueryContext) -> Result<EvalValue, String> {
    base_eval(args, context, |v: i64| {
        DateTime::from_timestamp_secs(v).unwrap().year() as f64
    })
}

pub static YEAR_FUNCTION_SPEC: FunctionSpec = FunctionSpec {
    name: "year",
    arg_types: &[],
    return_type: ExpressionType::InstantVector,
    eval: eval_year,
};

#[cfg(test)]
mod tests {
    use crate::core::{Sample, TimestampSecond};

    use super::*;

    #[test]
    fn test_time_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_time(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), ts.clone() as f64))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_day_of_month_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_day_of_month(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 1.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_day_of_week_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_day_of_week(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 5.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_day_of_year_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_day_of_year(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 121.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_days_in_month_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_days_in_month(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 31.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_hour_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_hour(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 15.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_minute_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_minute(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 0.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_month_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_month(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 5.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }

    #[test]
    fn test_year_function() {
        let timestamps: Vec<_> = (0..3).into_iter().map(|n| n + 894034800).collect();
        let context = QueryContext::new(timestamps.clone());

        let res = eval_year(vec![], context).unwrap();

        if let EvalValue::Instant(mut iter) = res {
            let actual_series = iter.next().unwrap().unwrap();
            assert_eq!(iter.next().unwrap(), None);

            assert_eq!(actual_series.labels.len(), 0);
            let expected: Vec<_> = timestamps
                .clone()
                .iter()
                .map(|ts| Sample::new(TimestampSecond(ts.clone()), 1998.0))
                .collect();
            assert_eq!(actual_series.samples, expected);
        } else {
            panic!("unexpected eval value");
        }
    }
}
