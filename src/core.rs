use chrono::{DateTime, Local};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Timestamp(f64);

impl Timestamp {
    pub fn parse(input: &str) -> Result<Self, String> {
        if let Ok(seconds) = input.parse::<f64>() {
            if seconds.is_finite() {
                return Ok(Self(seconds));
            }
        }
        if let Ok(timestamp) = DateTime::parse_from_rfc3339(input) {
            let seconds = timestamp.timestamp() as f64
                + f64::from(timestamp.timestamp_subsec_nanos()) / 1_000_000_000.0;

            return Ok(Self(seconds));
        }

        Err(format!("invalid timestamp: {input}"))
    }
    pub fn as_seconds(self) -> f64 {
        self.0
    }

    pub fn now() -> Self {
        let val = Local::now().timestamp() as f64;
        Timestamp(val)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Label {
    pub name: String,
    pub value: String,
}

impl Label {
    pub fn new(name: String, value: String) -> Self {
        Self { name, value }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Labels {
    labels: Vec<Label>,
}

impl Labels {
    pub fn from(mut labels: Vec<Label>) -> Result<Labels, String> {
        labels.sort_by(|left, right| left.name.cmp(&right.name));
        for pair in labels.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(format!("duplicate label name {}", pair[0].name));
            }
        }

        Ok(Self { labels })
    }

    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    pub fn merge(&self, other: &Labels) -> Result<Labels, String> {
        let labels = vec![self, other]
            .iter()
            .map(|ls| {
                ls.labels()
                    .iter()
                    .map(|l| l.clone())
                    .collect::<Vec<Label>>()
            })
            .collect::<Vec<Vec<Label>>>()
            .concat();
        Self::from(labels)
    }
}
