//! Segment template identifier substitution.
//!
//! Implements the `$Identifier$` / `$Identifier%0[width]d$` grammar with the
//! `$$` escape for a literal `$`.
//!
//! The `$SubNumber$` identifier is recognized and formatted, but v1 does not
//! iterate sub-segments: the segment sequence supplies no sub-number, so a
//! media template that references `$SubNumber$` is rejected with
//! [`ErrorKind::UnsupportedAddressing`]. Low-latency sub-segment addressing
//! (driven by `Resync`) is out of scope for this version.

use crate::error::{Error, ErrorKind};

/// The values available for substitution in one expansion.
///
/// A field left `None` means the identifier is not addressable in the current
/// context; referencing it then is an error rather than a silent blank.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Values<'a> {
    pub(crate) representation_id: &'a str,
    pub(crate) bandwidth: u32,
    pub(crate) number: Option<u64>,
    pub(crate) time: Option<u64>,
    pub(crate) sub_number: Option<u64>,
}

/// Expands `template` by substituting identifiers from `values`.
///
/// # Errors
///
/// Returns [`ErrorKind::InvalidTemplateFormat`] for an unterminated `$` or a
/// malformed format tag, [`ErrorKind::UnknownTemplateIdentifier`] for an
/// identifier DASH does not define, and [`ErrorKind::UnsupportedAddressing`]
/// when a defined identifier has no value in this context.
pub(crate) fn expand(template: &str, values: &Values<'_>, path: &str) -> Result<String, Error> {
    let invalid_format = || {
        Error::new(
            path.to_string(),
            ErrorKind::InvalidTemplateFormat {
                template: template.to_string(),
            },
        )
    };

    // `$` always comes in pairs, so splitting yields an odd number of parts:
    // even indices are literal text, odd indices are the spec between a pair.
    let parts: Vec<&str> = template.split('$').collect();
    if parts.len() & 1 == 0 {
        return Err(invalid_format());
    }

    let mut output = String::new();
    for (index, part) in parts.iter().enumerate() {
        if index & 1 == 0 {
            output.push_str(part);
        } else if part.is_empty() {
            output.push('$');
        } else {
            output.push_str(&substitute(part, values, path, &invalid_format)?);
        }
    }
    Ok(output)
}

fn substitute(
    spec: &str,
    values: &Values<'_>,
    path: &str,
    invalid_format: &impl Fn() -> Error,
) -> Result<String, Error> {
    let (identifier, format) = match spec.split_once('%') {
        Some((identifier, format)) => (identifier, Some(format)),
        None => (spec, None),
    };

    let unsupported = |reason: &str| {
        Error::new(
            path.to_string(),
            ErrorKind::UnsupportedAddressing {
                reason: reason.to_string(),
            },
        )
    };

    match identifier {
        "RepresentationID" => {
            if format.is_some() {
                // RepresentationID is a string; a numeric format tag is undefined for it.
                return Err(invalid_format());
            }
            Ok(values.representation_id.to_string())
        }
        "Bandwidth" => Ok(format_number(
            values.bandwidth.into(),
            format,
            invalid_format,
        )?),
        "Number" => {
            let number = values
                .number
                .ok_or_else(|| unsupported("$Number$ is not addressable here"))?;
            format_number(number, format, invalid_format)
        }
        "Time" => {
            let time = values
                .time
                .ok_or_else(|| unsupported("$Time$ requires a SegmentTimeline"))?;
            format_number(time, format, invalid_format)
        }
        "SubNumber" => {
            let sub_number = values.sub_number.ok_or_else(|| {
                unsupported("$SubNumber$ requires low-latency sub-segment addressing")
            })?;
            format_number(sub_number, format, invalid_format)
        }
        other => Err(Error::new(
            path.to_string(),
            ErrorKind::UnknownTemplateIdentifier {
                identifier: other.to_string(),
            },
        )),
    }
}

/// Applies a `%0[width]d` format tag, zero-padding to `width`.
fn format_number(
    value: u64,
    format: Option<&str>,
    invalid_format: &impl Fn() -> Error,
) -> Result<String, Error> {
    let Some(format) = format else {
        return Ok(value.to_string());
    };
    let width_text = format.strip_suffix('d').ok_or_else(invalid_format)?;
    // DASH writes the zero-pad flag and width together (`05`); a bare width
    // (`5`) is also accepted. Either way the conversion zero-pads.
    if !width_text
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        return Err(invalid_format());
    }
    let width = width_text.parse::<usize>().unwrap_or(0);
    Ok(format!("{value:0width$}"))
}
