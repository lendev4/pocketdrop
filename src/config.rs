use std::{env, fmt, path::PathBuf, str::FromStr};

pub struct Config {
    pub data_dir: PathBuf,
    pub default_expiry: Expiry,
    /// Choices offered when sending, always including the default.
    pub expiry_options: Vec<Expiry>,
    pub max_drops: usize,
    pub max_text_bytes: usize,
    pub max_file_bytes: u64,
    /// Cap on everything stored, text and files together.
    pub max_total_bytes: u64,
    /// Direct-transfer service offered for files over the size limit.
    pub pairdrop_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let default_expiry: Expiry = parse_env("DEFAULT_EXPIRY", "24h");
        let mut expiry_options: Vec<Expiry> = env::var("EXPIRY_OPTIONS")
            .unwrap_or_else(|_| "once,5m,10m,1h,24h,7d,never".to_owned())
            .split(',')
            .map(|option| option.trim().parse().expect("invalid EXPIRY_OPTIONS entry"))
            .collect();
        if !expiry_options.contains(&default_expiry) {
            expiry_options.push(default_expiry);
        }
        expiry_options.sort_by_key(|expiry| expiry.sort_key());

        Self {
            data_dir: env::var("DATA_DIR").unwrap_or_else(|_| "data".to_owned()).into(),
            default_expiry,
            expiry_options,
            max_drops: parse_env("MAX_DROPS", "50"),
            max_text_bytes: parse_env("MAX_TEXT_BYTES", "1048576"),
            max_file_bytes: parse_env("MAX_FILE_BYTES", "104857600"),
            max_total_bytes: parse_env("MAX_TOTAL_BYTES", "2147483648"),
            pairdrop_url: env::var("PAIRDROP_URL").unwrap_or_else(|_| "https://pairdrop.net/".to_owned()),
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

/// Seconds a consumed `once` drop stays downloadable, so an interrupted
/// download can be retried.
pub const ONCE_GRACE_SECS: u64 = 5 * 60;
/// Unused `once` drops are still removed after this long.
pub const ONCE_MAX_SECS: u64 = 7 * 86_400;

/// How long a drop is kept. Written as `10m`, `1h`, `7d`, `never` or `once`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Expiry {
    After(u64),
    Never,
    /// Deleted shortly after its first download or copy.
    Once,
}

impl Expiry {
    /// Seconds until the drop is deleted regardless of use.
    pub fn lifetime(self) -> Option<u64> {
        match self {
            Self::After(secs) => Some(secs),
            Self::Never => None,
            Self::Once => Some(ONCE_MAX_SECS),
        }
    }

    /// Wording for the send form; each choice reads on its own, without a label.
    pub fn describe(self) -> String {
        match self {
            Self::After(secs) => format!("Keep {}", Duration(secs)),
            Self::Never => "Keep until deleted".to_owned(),
            Self::Once => "Keep until used".to_owned(),
        }
    }

    /// Order in the send form: once, then shortest to longest, then never.
    fn sort_key(self) -> u64 {
        match self {
            Self::Once => 0,
            Self::After(secs) => secs,
            Self::Never => u64::MAX,
        }
    }
}

impl FromStr for Expiry {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, ()> {
        match value {
            "never" => return Ok(Self::Never),
            "once" => return Ok(Self::Once),
            _ => {}
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
            n => Ok(Self::After(n * factor)),
        }
    }
}

impl fmt::Display for Expiry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Never => f.write_str("never"),
            Self::Once => f.write_str("once"),
            // Days only from two days up, so 24h stays 24h.
            Self::After(secs) if secs >= 2 * 86_400 && secs % 86_400 == 0 => write!(f, "{}d", secs / 86_400),
            Self::After(secs) if secs % 3600 == 0 => write!(f, "{}h", secs / 3600),
            Self::After(secs) => write!(f, "{}m", secs / 60),
        }
    }
}

/// A byte count in words: "512 B", "1.4 MB".
pub struct Bytes(pub u64);

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
        if self.0 < 1024 {
            return write!(f, "{} B", self.0);
        }
        #[expect(clippy::cast_precision_loss, reason = "display only")]
        let mut value = self.0 as f64 / 1024.0;
        let mut unit = 0;
        while value >= 1024.0 && unit < UNITS.len() - 1 {
            value /= 1024.0;
            unit += 1;
        }
        if value < 10.0 {
            write!(f, "{value:.1} {}", UNITS[unit])
        } else {
            write!(f, "{value:.0} {}", UNITS[unit])
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
        for text in ["10m", "1h", "24h", "7d", "never", "once"] {
            assert_eq!(text.parse::<Expiry>().unwrap().to_string(), text);
        }
        assert!("0h".parse::<Expiry>().is_err());
        assert!("5x".parse::<Expiry>().is_err());
        assert!("".parse::<Expiry>().is_err());
    }

    #[test]
    fn bytes_read_naturally() {
        assert_eq!(Bytes(512).to_string(), "512 B");
        assert_eq!(Bytes(1536).to_string(), "1.5 KB");
        assert_eq!(Bytes(25 * 1024 * 1024).to_string(), "25 MB");
        assert_eq!(Bytes(2 * 1024 * 1024 * 1024).to_string(), "2.0 GB");
    }
}
