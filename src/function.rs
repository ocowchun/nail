mod absent;
mod clamp;
mod math;
mod quantile;
mod rate;
mod time;
mod types;
mod vector;

pub use types::EvalValue;
pub use types::FunctionSpec;
pub use types::QueryContext;

static FUNCTIONS: &[&FunctionSpec] = &[
    &absent::ABSENT_FUNCTION_SPEC,
    &absent::ABSENT_OVER_TIME_FUNCTION_SPEC,
    &clamp::CLAMP_FUNCTION_SPEC,
    &clamp::CLAMP_MAX_FUNCTION_SPEC,
    &clamp::CLAMP_MIN_FUNCTION_SPEC,
    &math::ABS_FUNCTION_SPEC,
    &math::CEIL_FUNCTION_SPEC,
    &math::FLOOR_FUNCTION_SPEC,
    &math::LN_FUNCTION_SPEC,
    &math::LOG2_FUNCTION_SPEC,
    &math::LOG10_FUNCTION_SPEC,
    &math::ROUND_FUNCTION_SPEC,
    &math::SQRT_FUNCTION_SPEC,
    &rate::RATE_FUNCTION_SPEC,
    &rate::INCREASE_FUNCTION_SPEC,
    &rate::DELTA_FUNCTION_SPEC,
    &quantile::HISTOGRAM_QUANTILE_FUNCTION_SPEC,
    &time::TIME_FUNCTION_SPEC,
    &time::DAY_OF_MONTH_FUNCTION_SPEC,
    &time::DAY_OF_WEEK_FUNCTION_SPEC,
    &time::DAY_OF_YEAR_FUNCTION_SPEC,
    &time::DAYS_IN_MONTH_FUNCTION_SPEC,
    &time::HOUR_FUNCTION_SPEC,
    &time::MINUTE_FUNCTION_SPEC,
    &time::MONTH_FUNCTION_SPEC,
    &time::YEAR_FUNCTION_SPEC,
    &vector::VECTOR_FUNCTION_SPEC,
];

pub fn find_function_spec(name: &str) -> Option<&'static FunctionSpec> {
    FUNCTIONS.iter().copied().find(|spec| spec.name == name)
}
