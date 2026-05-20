use crate::core::errors::{BitOSDTError, BitOSDTResult};

const EN_US_INPUT_LOCALE: &str = "0409:00000409";
const EN_GB_INPUT_LOCALE: &str = "0809:00000809";

fn invalid_locale_error(input: &str) -> BitOSDTError {
    BitOSDTError::InvalidInput(format!(
        "Invalid language '{}'. Expected BCP-47 locale like 'en-US', 'fr-FR', or 'zh-Hant-TW'.",
        input
    ))
}

fn title_case_ascii(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut output = String::new();
    output.push(first.to_ascii_uppercase());
    for c in chars {
        output.push(c.to_ascii_lowercase());
    }
    output
}

fn is_alpha_len(value: &str, min: usize, max: usize) -> bool {
    let len = value.len();
    len >= min && len <= max && value.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_script_subtag(value: &str) -> bool {
    is_alpha_len(value, 4, 4)
}

fn is_region_subtag(value: &str) -> bool {
    (is_alpha_len(value, 2, 2)) || (value.len() == 3 && value.chars().all(|c| c.is_ascii_digit()))
}

fn is_variant_subtag(value: &str) -> bool {
    if value.is_empty() || !value.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }

    let len = value.len();
    if (5..=8).contains(&len) {
        return true;
    }

    len == 4
        && value
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
}

pub fn normalize_language_tag(language: &str) -> BitOSDTResult<String> {
    let trimmed = language.trim();
    if trimmed.is_empty() {
        return Err(invalid_locale_error(language));
    }

    let normalized = trimmed.replace('_', "-");
    if normalized.starts_with('-') || normalized.ends_with('-') || normalized.contains("--") {
        return Err(invalid_locale_error(language));
    }

    let lowered = normalized.to_ascii_lowercase();
    let lowered = if lowered.len() == 4
        && !lowered.contains('-')
        && lowered.chars().all(|c| c.is_ascii_alphabetic())
    {
        format!("{}-{}", &lowered[0..2], &lowered[2..4])
    } else {
        lowered
    };

    let mut parts = lowered.split('-');
    let Some(primary) = parts.next() else {
        return Err(invalid_locale_error(language));
    };

    if !is_alpha_len(primary, 2, 3) {
        return Err(invalid_locale_error(language));
    }

    let mut script: Option<String> = None;
    let mut region: Option<String> = None;
    let mut variants: Vec<String> = Vec::new();

    for part in parts {
        if part.is_empty() {
            return Err(invalid_locale_error(language));
        }

        if script.is_none() && region.is_none() && is_script_subtag(part) {
            script = Some(title_case_ascii(part));
            continue;
        }

        if region.is_none() && is_region_subtag(part) {
            region = Some(if part.len() == 2 {
                part.to_ascii_uppercase()
            } else {
                part.to_string()
            });
            continue;
        }

        if is_variant_subtag(part) {
            variants.push(part.to_ascii_lowercase());
            continue;
        }

        return Err(invalid_locale_error(language));
    }

    let mut canonical_parts: Vec<String> = Vec::new();
    canonical_parts.push(primary.to_ascii_lowercase());
    if let Some(script) = script {
        canonical_parts.push(script);
    }
    if let Some(region) = region {
        canonical_parts.push(region);
    }
    canonical_parts.extend(variants);

    Ok(canonical_parts.join("-"))
}

pub fn resolve_unattend_locale_settings(language: &str) -> BitOSDTResult<(String, String)> {
    let canonical_language = normalize_language_tag(language)?;
    let input_locale = match canonical_language.as_str() {
        "en-US" => EN_US_INPUT_LOCALE.to_string(),
        "en-GB" => EN_GB_INPUT_LOCALE.to_string(),
        _ => canonical_language.clone(),
    };

    Ok((canonical_language, input_locale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_language_tag_accepts_expected_valid_values() {
        assert_eq!(normalize_language_tag("en-us").unwrap(), "en-US");
        assert_eq!(normalize_language_tag("en_gb").unwrap(), "en-GB");
        assert_eq!(normalize_language_tag("fr-fr").unwrap(), "fr-FR");
        assert_eq!(normalize_language_tag("fr_FR").unwrap(), "fr-FR");
        assert_eq!(normalize_language_tag("zh-hant-tw").unwrap(), "zh-Hant-TW");
        assert_eq!(normalize_language_tag("ptbr").unwrap(), "pt-BR");
    }

    #[test]
    fn normalize_language_tag_rejects_invalid_values() {
        for invalid in ["", "fr--fr", "x", "12-AB", "fr-"] {
            assert!(
                normalize_language_tag(invalid).is_err(),
                "{} should fail",
                invalid
            );
        }
    }

    #[test]
    fn resolve_unattend_locale_settings_keeps_english_keyboard_ids() {
        assert_eq!(
            resolve_unattend_locale_settings("en-US").unwrap(),
            ("en-US".to_string(), EN_US_INPUT_LOCALE.to_string())
        );
        assert_eq!(
            resolve_unattend_locale_settings("en-GB").unwrap(),
            ("en-GB".to_string(), EN_GB_INPUT_LOCALE.to_string())
        );
    }

    #[test]
    fn resolve_unattend_locale_settings_uses_canonical_tag_for_non_english() {
        assert_eq!(
            resolve_unattend_locale_settings("fr-fr").unwrap(),
            ("fr-FR".to_string(), "fr-FR".to_string())
        );
    }
}
