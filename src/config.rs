use std::{env, fmt, path::PathBuf, str::FromStr};

pub struct Config {
    pub data_dir: PathBuf,
    pub default_expiry: Expiry,
    /// Choices offered when sending, always including the default.
    pub expiry_options: Vec<Expiry>,
    pub max_drops: usize,
    pub max_text_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        let default_expiry: Expiry = parse_env("DEFAULT_EXPIRY", "24h");
        let mut expiry_options: Vec<Expiry> = env::var("EXPIRY_OPTIONS")
            .unwrap_or_else(|_| "10m,1h,24h,7d,never".to_owned())
            .split(',')
            .map(|option| option.trim().parse().expect("invalid EXPIRY_OPTIONS entry"))
            .collect();
        if !expiry_options.contains(&default_expiry) {
            expiry_options.push(default_expiry);
        }
        expiry_options.sort_by_key(|expiry| expiry.seconds().unwrap_or(u64::MAX));

        Self {
            data_dir: env::var("DATA_DIR").unwrap_or_else(|_| "data".to_owned()).into(),
            default_expiry,
            expiry_options,
            max_drops: parse_env("MAX_DROPS", "50"),
            max_text_bytes: parse_env("MAX_TEXT_BYTES", "1048576"),
        }
    }

    /// The submitted expiry if it is one of the offered options, else the default.
    pub fn expiry_or_default(&self, submitted: Option<&str>) -> Expiry {
        submitted
            .and_then(|value| value.parse().ok())
            .filter(|expiry| self.expiry_options.contains(expiry))
            .unwrap_or(self.default_expiry)
    }
}

fn parse_env<T: FromStr>(name: &str, default: &str) -> T {
    let value = env::var(name).unwrap_or_else(|_| default.to_owned());
    value
        .parse()
        .unwrap_or_else(|_| panic!("{name} has an invalid value: {value}"))
}

/// How long a drop is kept: a duration in seconds, or `None` for until deleted.
/// Written as `10m`, `1h`, `7d` or `never`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Expiry(Option<u64>);

impl Expiry {
    pub fn seconds(self) -> Option<u64> {
        self.0
    }

    /// Wording for the send form.
    pub fn describe(self) -> String {
        match self.0 {
            None => "Until deleted".to_owned(),
            Some(secs) => format!("{}", Duration(secs)),
        }
    }
}

impl FromStr for Expiry {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, ()> {
        if value == "never" {
            return Ok(Self(None));
        }
        let (number, unit) = value.split_at(value.len().saturating_sub(1));
        let number: u64 = number.parse().map_err(|_| ())?;
        let factor = match unit {
            "m" => 60,
            "h" => 3600,
            "d" => 86_400,
            _ => return Err(()),
        };
        match number {
            0 => Err(()),
            n => Ok(Self(Some(n * factor))),
        }
    }
}

impl fmt::Display for Expiry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => f.write_str("never"),
            // Days only from two days up, so 24h stays 24h.
            Some(secs) if secs >= 2 * 86_400 && secs % 86_400 == 0 => write!(f, "{}d", secs / 86_400),
            Some(secs) if secs % 3600 == 0 => write!(f, "{}h", secs / 3600),
            Some(secs) => write!(f, "{}m", secs / 60),
        }
    }
}

/// A span of seconds in words, picking the largest fitting unit: "10 min", "7 days".
pub struct Duration(pub u64);

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.0;
        let (n, unit) = match secs {
            0..3600 => ((secs / 60).max(1), "min"),
            3600..172_800 => (secs / 3600, "hour"),
            _ => (secs / 86_400, "day"),
        };
        let plural = if n == 1 || unit == "min" { "" } else { "s" };
        write!(f, "{n} {unit}{plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_round_trips() {
        for text in ["10m", "1h", "24h", "7d", "never"] {
            assert_eq!(text.parse::<Expiry>().unwrap().to_string(), text);
        }
        assert!("0h".parse::<Expiry>().is_err());
        assert!("5x".parse::<Expiry>().is_err());
        assert!("".parse::<Expiry>().is_err());
    }
}
