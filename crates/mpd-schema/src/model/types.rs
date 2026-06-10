//! Attribute value types for the simple types defined by `DASH-MPD.xsd`.
//!
//! `xs:dateTime` attributes are represented with [`chrono`] types and need no
//! custom type here.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use crate::error::{Error, ErrorKind};

fn invalid_value(value: &str, expected: &str) -> Error {
    Error::new(ErrorKind::InvalidValue {
        value: value.to_string(),
        expected: expected.to_string(),
    })
}

fn parse_unsigned_digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// An `xs:duration` value.
///
/// Unlike [`std::time::Duration`], an `xs:duration` may carry a year-month
/// component that has no fixed length in seconds, so this type stores the
/// XSD value space directly: a month count and a second count, either of
/// which may be zero, plus a sign.
///
/// Parsing normalizes the lexical form (the deterministic conversions
/// `1Y = 12M` and `1D = 24H = 1440M = 86400S` are applied), so `PT2M` and
/// `PT120S` compare equal. [`fmt::Display`] produces the canonical form.
///
/// Fractional seconds are held at nanosecond precision. Digits beyond the
/// ninth are accepted only when they are all zero (no information is lost);
/// anything finer is rejected rather than silently rounded (ADR-0008).
///
/// ```
/// use mpd_schema::model::types::XsDuration;
///
/// let duration: XsDuration = "PT120S".parse()?;
/// assert_eq!(duration.seconds, 120);
/// assert_eq!(duration.to_string(), "PT2M");
/// # Ok::<(), mpd_schema::error::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct XsDuration {
    /// Whether the duration is negative.
    ///
    /// A zero duration is never negative; parsing normalizes `-PT0S`.
    pub negative: bool,
    /// The year-month component, in months.
    pub months: u64,
    /// The day-time component, in whole seconds.
    pub seconds: u64,
    /// The fractional-second component, in nanoseconds (`0..1_000_000_000`).
    pub nanoseconds: u32,
}

impl XsDuration {
    /// Creates a zero duration.
    pub fn new() -> Self {
        Self::default()
    }
}

impl FromStr for XsDuration {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        const EXPECTED: &str = "an `xs:duration` such as `PT30S` or `-P1DT2H`";
        let invalid = || invalid_value(input, EXPECTED);

        let unsigned = input.strip_prefix('-');
        let negative = unsigned.is_some();
        let body = unsigned.unwrap_or(input);
        let body = body.strip_prefix('P').ok_or_else(invalid)?;
        if body.is_empty() {
            return Err(invalid());
        }

        let mut months: u64 = 0;
        let mut seconds: u64 = 0;
        let mut nanoseconds: u32 = 0;
        let mut in_time = false;
        let mut previous_position: u8 = 0;
        let mut characters = body.chars().peekable();

        while let Some(&character) = characters.peek() {
            if character == 'T' {
                if in_time {
                    return Err(invalid());
                }
                in_time = true;
                characters.next();
                if characters.peek().is_none() {
                    return Err(invalid());
                }
                continue;
            }

            let mut number: u64 = 0;
            let mut has_digits = false;
            while let Some(digit) = characters.peek().and_then(|c| c.to_digit(10)) {
                number = number
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u64::from(digit)))
                    .ok_or_else(invalid)?;
                has_digits = true;
                characters.next();
            }
            if !has_digits {
                return Err(invalid());
            }

            let mut fraction: Option<u32> = None;
            if characters.peek() == Some(&'.') {
                characters.next();
                let mut value: u32 = 0;
                let mut digit_count: u32 = 0;
                while let Some(digit) = characters.peek().and_then(|c| c.to_digit(10)) {
                    if digit_count < 9 {
                        value = value
                            .checked_mul(10)
                            .and_then(|v| v.checked_add(digit))
                            .ok_or_else(invalid)?;
                    } else if digit != 0 {
                        // 10桁目以降は情報落ちのないゼロのみ受理する（ADR-0008）
                        return Err(invalid());
                    }
                    digit_count = digit_count.checked_add(1).ok_or_else(invalid)?;
                    characters.next();
                }
                if digit_count == 0 {
                    return Err(invalid());
                }
                let scale = 9u32
                    .checked_sub(digit_count.min(9))
                    .and_then(|exponent| 10u32.checked_pow(exponent))
                    .ok_or_else(invalid)?;
                fraction = Some(value.checked_mul(scale).ok_or_else(invalid)?);
            }

            let designator = characters.next().ok_or_else(invalid)?;
            if fraction.is_some() && designator != 'S' {
                return Err(invalid());
            }
            let position: u8 = match (in_time, designator) {
                (false, 'Y') => 1,
                (false, 'M') => 2,
                (false, 'D') => 3,
                (true, 'H') => 4,
                (true, 'M') => 5,
                (true, 'S') => 6,
                _ => return Err(invalid()),
            };
            if position <= previous_position {
                return Err(invalid());
            }
            previous_position = position;

            match position {
                1 => {
                    months = number
                        .checked_mul(12)
                        .and_then(|n| months.checked_add(n))
                        .ok_or_else(invalid)?;
                }
                2 => months = months.checked_add(number).ok_or_else(invalid)?,
                3 => {
                    seconds = number
                        .checked_mul(86_400)
                        .and_then(|n| seconds.checked_add(n))
                        .ok_or_else(invalid)?;
                }
                4 => {
                    seconds = number
                        .checked_mul(3_600)
                        .and_then(|n| seconds.checked_add(n))
                        .ok_or_else(invalid)?;
                }
                5 => {
                    seconds = number
                        .checked_mul(60)
                        .and_then(|n| seconds.checked_add(n))
                        .ok_or_else(invalid)?;
                }
                _ => {
                    seconds = seconds.checked_add(number).ok_or_else(invalid)?;
                    nanoseconds = fraction.unwrap_or(0);
                }
            }
        }

        let negative = negative && !(months == 0 && seconds == 0 && nanoseconds == 0);
        Ok(Self {
            negative,
            months,
            seconds,
            nanoseconds,
        })
    }
}

impl fmt::Display for XsDuration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.months == 0 && self.seconds == 0 && self.nanoseconds == 0 {
            return formatter.write_str("PT0S");
        }
        if self.negative {
            formatter.write_str("-")?;
        }
        formatter.write_str("P")?;
        let years = self.months / 12;
        let months = self.months % 12;
        if years > 0 {
            write!(formatter, "{years}Y")?;
        }
        if months > 0 {
            write!(formatter, "{months}M")?;
        }
        let days = self.seconds / 86_400;
        let mut remainder = self.seconds % 86_400;
        if days > 0 {
            write!(formatter, "{days}D")?;
        }
        let hours = remainder / 3_600;
        remainder %= 3_600;
        let minutes = remainder / 60;
        let seconds = remainder % 60;
        if hours > 0 || minutes > 0 || seconds > 0 || self.nanoseconds > 0 {
            formatter.write_str("T")?;
            if hours > 0 {
                write!(formatter, "{hours}H")?;
            }
            if minutes > 0 {
                write!(formatter, "{minutes}M")?;
            }
            if self.nanoseconds > 0 {
                let padded = format!("{:09}", self.nanoseconds);
                write!(formatter, "{seconds}.{}S", padded.trim_end_matches('0'))?;
            } else if seconds > 0 {
                write!(formatter, "{seconds}S")?;
            }
        }
        Ok(())
    }
}

/// A frame rate, as written in attributes of XSD type `FrameRateType`.
///
/// The lexical form is `30` or `30000/1001`; an omitted denominator means 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FrameRate {
    /// The number of frames.
    pub numerator: u32,
    /// The number of seconds in which [`FrameRate::numerator`] frames are
    /// displayed.
    pub denominator: NonZeroU32,
}

impl FrameRate {
    /// Creates an integer frame rate (denominator 1).
    pub fn new(numerator: u32) -> Self {
        Self {
            numerator,
            denominator: NonZeroU32::MIN,
        }
    }
}

impl FromStr for FrameRate {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        const EXPECTED: &str = "a frame rate such as `30` or `30000/1001`";
        let invalid = || invalid_value(input, EXPECTED);

        let (numerator_text, denominator_text) = match input.split_once('/') {
            Some(parts) => parts,
            None => (input, "1"),
        };
        let numerator = parse_unsigned_digits(numerator_text).ok_or_else(invalid)?;
        if denominator_text.starts_with('0') {
            return Err(invalid());
        }
        let denominator = parse_unsigned_digits(denominator_text)
            .and_then(NonZeroU32::new)
            .ok_or_else(invalid)?;
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator.get() == 1 {
            write!(formatter, "{}", self.numerator)
        } else {
            write!(formatter, "{}/{}", self.numerator, self.denominator)
        }
    }
}

/// A ratio, as written in attributes of XSD type `RatioType` (for example
/// `par="16:9"`).
///
/// The XSD pattern `[0-9]*:[0-9]*` allows either side to be omitted, hence
/// both fields are optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct Ratio {
    /// The part before the colon.
    pub numerator: Option<u32>,
    /// The part after the colon.
    pub denominator: Option<u32>,
}

impl Ratio {
    /// Creates a ratio with both sides omitted.
    pub fn new() -> Self {
        Self::default()
    }
}

impl FromStr for Ratio {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        const EXPECTED: &str = "a ratio such as `16:9`";
        let invalid = || invalid_value(input, EXPECTED);

        let (numerator_text, denominator_text) = input.split_once(':').ok_or_else(invalid)?;
        let numerator = if numerator_text.is_empty() {
            None
        } else {
            Some(parse_unsigned_digits(numerator_text).ok_or_else(invalid)?)
        };
        let denominator = if denominator_text.is_empty() {
            None
        } else {
            Some(parse_unsigned_digits(denominator_text).ok_or_else(invalid)?)
        };
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(numerator) = self.numerator {
            write!(formatter, "{numerator}")?;
        }
        formatter.write_str(":")?;
        if let Some(denominator) = self.denominator {
            write!(formatter, "{denominator}")?;
        }
        Ok(())
    }
}

/// A value of XSD type `ConditionalUintType`, the union of `xs:unsignedInt`
/// and `xs:boolean`.
///
/// Following XSD union semantics, member types are tried in order: `0` and
/// `1` parse as [`ConditionalUint::Unsigned`], not as booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionalUint {
    /// The `xs:boolean` form (`true` or `false`).
    Boolean(bool),
    /// The `xs:unsignedInt` form.
    Unsigned(u32),
}

impl FromStr for ConditionalUint {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        const EXPECTED: &str = "`true`, `false`, or an unsigned integer";
        match input {
            "true" => Ok(Self::Boolean(true)),
            "false" => Ok(Self::Boolean(false)),
            _ => {
                let digits = input.strip_prefix('+').unwrap_or(input);
                parse_unsigned_digits(digits)
                    .map(Self::Unsigned)
                    .ok_or_else(|| invalid_value(input, EXPECTED))
            }
        }
    }
}

impl fmt::Display for ConditionalUint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(value) => write!(formatter, "{value}"),
            Self::Unsigned(value) => write!(formatter, "{value}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duration(input: &str) -> XsDuration {
        input.parse().unwrap()
    }

    #[test]
    fn duration_normalizes_minutes_and_seconds_to_the_same_value() {
        assert_eq!(duration("PT2M"), duration("PT120S"));
        assert_eq!(duration("PT2M").seconds, 120);
    }

    #[test]
    fn duration_display_is_canonical() {
        assert_eq!(duration("PT120S").to_string(), "PT2M");
        assert_eq!(duration("PT90061S").to_string(), "P1DT1H1M1S");
        assert_eq!(duration("P13M").to_string(), "P1Y1M");
    }

    #[test]
    fn duration_keeps_year_month_separate_from_day_time() {
        let parsed = duration("P1Y2M3DT4H5M6S");
        assert_eq!(parsed.months, 14);
        assert_eq!(parsed.seconds, 3 * 86_400 + 4 * 3_600 + 5 * 60 + 6);
        assert_eq!(parsed.to_string(), "P1Y2M3DT4H5M6S");
    }

    #[test]
    fn duration_parses_fractional_seconds() {
        let parsed = duration("PT1.5S");
        assert_eq!(parsed.seconds, 1);
        assert_eq!(parsed.nanoseconds, 500_000_000);
        assert_eq!(parsed.to_string(), "PT1.5S");

        assert_eq!(duration("PT0.000000001S").nanoseconds, 1);
        assert_eq!(duration("PT0.5S").to_string(), "PT0.5S");
    }

    #[test]
    fn duration_accepts_lossless_fraction_digits_beyond_nanoseconds() {
        assert_eq!(duration("PT1.5000000000S"), duration("PT1.5S"));
        assert_eq!(duration("PT1.000000000000S"), duration("PT1S"));
    }

    #[test]
    fn duration_parses_negative_values() {
        let parsed = duration("-P1DT2H");
        assert!(parsed.negative);
        assert_eq!(parsed.to_string(), "-P1DT2H");
    }

    #[test]
    fn duration_normalizes_negative_zero() {
        let parsed = duration("-PT0S");
        assert!(!parsed.negative);
        assert_eq!(parsed, XsDuration::new());
        assert_eq!(parsed.to_string(), "PT0S");
    }

    #[test]
    fn duration_zero_displays_as_pt0s() {
        assert_eq!(XsDuration::new().to_string(), "PT0S");
        assert_eq!(duration("P0D").to_string(), "PT0S");
    }

    #[test]
    fn duration_day_only_has_no_time_part() {
        assert_eq!(duration("P1D").to_string(), "P1D");
    }

    #[test]
    fn duration_rejects_malformed_input() {
        let inputs = [
            "",
            "P",
            "PT",
            "-P",
            "1Y",
            "P1S",
            "PT1Y",
            "P1H",
            "PT1M2H",
            "P1M1Y",
            "P1Y1Y",
            "PT1.5M",
            "P1.5D",
            "PT.5S",
            "PT1.S",
            "PT1Sx",
            "P1DT",
            "PT1.1234567891S",
            "PT1.0000000005S",
            "P99999999999999999999Y",
            " PT1S",
            "pt1s",
        ];
        for input in inputs {
            assert!(
                input.parse::<XsDuration>().is_err(),
                "expected `{input}` to be rejected"
            );
        }
    }

    #[test]
    fn frame_rate_parses_integer_form() {
        let parsed: FrameRate = "30".parse().unwrap();
        assert_eq!(parsed, FrameRate::new(30));
        assert_eq!(parsed.to_string(), "30");
    }

    #[test]
    fn frame_rate_parses_rational_form() {
        let parsed: FrameRate = "30000/1001".parse().unwrap();
        assert_eq!(parsed.numerator, 30_000);
        assert_eq!(parsed.denominator.get(), 1_001);
        assert_eq!(parsed.to_string(), "30000/1001");
    }

    #[test]
    fn frame_rate_normalizes_explicit_denominator_one() {
        let parsed: FrameRate = "25/1".parse().unwrap();
        assert_eq!(parsed.to_string(), "25");
    }

    #[test]
    fn frame_rate_rejects_malformed_input() {
        let inputs = ["", "30/0", "30/01", "30/", "/1001", "-30", "+30", "30.0"];
        for input in inputs {
            assert!(
                input.parse::<FrameRate>().is_err(),
                "expected `{input}` to be rejected"
            );
        }
    }

    #[test]
    fn ratio_parses_both_sides() {
        let parsed: Ratio = "16:9".parse().unwrap();
        assert_eq!(parsed.numerator, Some(16));
        assert_eq!(parsed.denominator, Some(9));
        assert_eq!(parsed.to_string(), "16:9");
    }

    #[test]
    fn ratio_allows_omitted_sides() {
        let parsed: Ratio = "16:".parse().unwrap();
        assert_eq!(parsed.numerator, Some(16));
        assert_eq!(parsed.denominator, None);
        assert_eq!(parsed.to_string(), "16:");

        let parsed: Ratio = ":".parse().unwrap();
        assert_eq!(parsed, Ratio::new());
        assert_eq!(parsed.to_string(), ":");
    }

    #[test]
    fn ratio_rejects_malformed_input() {
        let inputs = ["", "16", "16:9:3", "a:9", "16:b", "-16:9", "16: 9"];
        for input in inputs {
            assert!(
                input.parse::<Ratio>().is_err(),
                "expected `{input}` to be rejected"
            );
        }
    }

    #[test]
    fn conditional_uint_prefers_unsigned_over_boolean() {
        assert_eq!(
            "0".parse::<ConditionalUint>().unwrap(),
            ConditionalUint::Unsigned(0)
        );
        assert_eq!(
            "1".parse::<ConditionalUint>().unwrap(),
            ConditionalUint::Unsigned(1)
        );
    }

    #[test]
    fn conditional_uint_parses_boolean_words() {
        assert_eq!(
            "true".parse::<ConditionalUint>().unwrap(),
            ConditionalUint::Boolean(true)
        );
        assert_eq!(
            "false".parse::<ConditionalUint>().unwrap(),
            ConditionalUint::Boolean(false)
        );
    }

    #[test]
    fn conditional_uint_parses_boundary_values() {
        assert_eq!(
            "4294967295".parse::<ConditionalUint>().unwrap(),
            ConditionalUint::Unsigned(u32::MAX)
        );
        assert!("4294967296".parse::<ConditionalUint>().is_err());
        assert!("-1".parse::<ConditionalUint>().is_err());
    }

    #[test]
    fn conditional_uint_displays_lexical_form() {
        assert_eq!(ConditionalUint::Boolean(true).to_string(), "true");
        assert_eq!(ConditionalUint::Unsigned(0).to_string(), "0");
    }

    #[test]
    fn invalid_value_error_carries_the_input() {
        let error = "abc".parse::<XsDuration>().unwrap_err();
        match error.kind {
            ErrorKind::InvalidValue { value, .. } => assert_eq!(value, "abc"),
            other => panic!("unexpected error kind: {other:?}"),
        }
    }
}
