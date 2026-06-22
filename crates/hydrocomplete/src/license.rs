//! Pro license activation — mirrors `LicenseActivator.cs`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const DEFAULT_VALIDATE_URL: &str =
    "https://hc-refactored.fly.dev/api/licensing/validate";

pub const TOKEN_PREFIX: &str = "hc_live_";

pub const STUB_VALIDITY_DAYS: u64 = 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseValidationMode {
    None,
    Online,
    OfflineStub,
    DevBypass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseRecord {
    pub email: String,
    pub token: String,
    pub expires: String,
    #[serde(default)]
    pub last_validated: String,
    #[serde(default, rename = "validationMode")]
    pub validation_mode: String,
}

#[derive(Debug, Clone)]
pub struct LicenseActivationResult {
    pub success: bool,
    pub message: String,
    pub mode: LicenseValidationMode,
    pub expires: String,
}

pub fn license_file_path() -> PathBuf {
    if let Some(base) = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    {
        base.join("HydroComplete").join("license.json")
    } else {
        PathBuf::from(".hydrocomplete-license.json")
    }
}

pub fn is_dev_bypass_enabled() -> bool {
    std::env::var("HYDROCOMPLETE_PRO")
        .map(|v| v == "1")
        .unwrap_or(false)
}

pub fn is_well_formed_token(token: &str) -> bool {
    let trimmed = token.trim();
    trimmed.starts_with(TOKEN_PREFIX) && trimmed.len() >= TOKEN_PREFIX.len() + 8
}

pub fn try_parse_combined_input(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    let space = trimmed.find(' ')?;
    let email = trimmed[..space].trim();
    let token = trimmed[space + 1..].trim();
    if email.contains('@') && is_well_formed_token(token) {
        Some((email.to_string(), token.to_string()))
    } else {
        None
    }
}

pub fn try_read_license(path: &Path) -> Option<LicenseRecord> {
    let json = std::fs::read_to_string(path).ok()?;
    let record: LicenseRecord = serde_json::from_str(&json).ok()?;
    if !is_license_fields_valid(&record) {
        return None;
    }
    let expires = parse_rfc3339(&record.expires)?;
    if expires <= SystemTime::now() {
        return None;
    }
    Some(record)
}

pub fn try_read_license_metadata(path: &Path) -> Option<LicenseRecord> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

pub fn write_license_file(path: &Path, record: &LicenseRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

pub struct LicenseActivator {
    validate_url: String,
}

impl Default for LicenseActivator {
    fn default() -> Self {
        Self::new()
    }
}

impl LicenseActivator {
    pub fn new() -> Self {
        Self {
            validate_url: DEFAULT_VALIDATE_URL.into(),
        }
    }

    pub fn with_validate_url(mut self, url: impl Into<String>) -> Self {
        self.validate_url = url.into();
        self
    }

    pub fn activate(&self, email: &str, token: &str, license_path: &Path) -> LicenseActivationResult {
        let email = email.trim();
        let token = token.trim();
        if email.is_empty() || !email.contains('@') {
            return fail("Enter a valid email address.");
        }
        if !is_well_formed_token(token) {
            return fail(format!(
                "Activation token must start with {TOKEN_PREFIX} and be at least {} characters.",
                TOKEN_PREFIX.len() + 8
            ));
        }
        self.activate_core(email, token, license_path)
    }

    pub fn refresh(&self, license_path: &Path) -> LicenseActivationResult {
        let Some(existing) = try_read_license_metadata(license_path) else {
            return fail("No license file to validate. Run HC_ACTIVATE first.");
        };
        if existing.email.is_empty() || existing.token.is_empty() {
            return fail("No license file to validate. Run HC_ACTIVATE first.");
        }
        self.activate_core(&existing.email, &existing.token, license_path)
    }

    fn activate_core(&self, email: &str, token: &str, license_path: &Path) -> LicenseActivationResult {
        let online = self.try_online_validation(email, token);
        if online.success {
            if let Some(record) = online.record {
                let _ = write_license_file(license_path, &record);
                return LicenseActivationResult {
                    success: true,
                    message: "Pro activated (online validation).".into(),
                    mode: LicenseValidationMode::Online,
                    expires: record.expires,
                };
            }
        }
        if online.server_said_invalid {
            return fail(
                online
                    .error_message
                    .unwrap_or_else(|| "License is not valid on the server. Contact support.".into()),
            );
        }
        if !is_well_formed_token(token) {
            return fail(
                online
                    .error_message
                    .unwrap_or_else(|| "Online validation failed and token format is invalid.".into()),
            );
        }
        let stub = build_offline_stub_record(email, token);
        let _ = write_license_file(license_path, &stub);
        let message = if online.was_network_attempt {
            "Pro activated (offline stub — hydrocomplete.com unreachable or token not in server registry)."
        } else {
            "Pro activated (offline stub — beta token accepted locally)."
        };
        LicenseActivationResult {
            success: true,
            message: message.into(),
            mode: LicenseValidationMode::OfflineStub,
            expires: stub.expires.clone(),
        }
    }

    fn try_online_validation(&self, email: &str, token: &str) -> OnlineValidationAttempt {
        let body = serde_json::json!({
            "licenseKey": token,
            "features": ["reports", "export", "civil3d"],
        });
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(15))
            .user_agent("HydroComplete-OpenCAD/0.4")
            .build();
        match agent
            .post(&self.validate_url)
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(resp) => {
                let status = resp.status();
                let response_body = resp.into_string().unwrap_or_default();
                if status >= 400 {
                    return OnlineValidationAttempt {
                        was_network_attempt: true,
                        error_message: Some(format!("Server returned {status}.")),
                        ..Default::default()
                    };
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&response_body) else {
                    return OnlineValidationAttempt {
                        was_network_attempt: true,
                        error_message: Some("Invalid server response.".into()),
                        ..Default::default()
                    };
                };
                if !v.get("valid").and_then(|x| x.as_bool()).unwrap_or(false) {
                    return OnlineValidationAttempt {
                        was_network_attempt: true,
                        server_said_invalid: true,
                        error_message: read_error_message(&v),
                        ..Default::default()
                    };
                }
                let expires = read_expires(&v).unwrap_or_else(|| {
                    (SystemTime::now() + Duration::from_secs(STUB_VALIDITY_DAYS * 86400))
                        .duration_since(UNIX_EPOCH)
                        .map(|d| format_iso8601(d.as_secs()))
                        .unwrap_or_default()
                });
                let stored_token = v
                    .get("accessToken")
                    .and_then(|x| x.as_str())
                    .unwrap_or(token)
                    .to_string();
                let now = format_iso8601(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                );
                OnlineValidationAttempt {
                    success: true,
                    was_network_attempt: true,
                    record: Some(LicenseRecord {
                        email: email.to_string(),
                        token: stored_token,
                        expires,
                        last_validated: now,
                        validation_mode: "online".into(),
                    }),
                    ..Default::default()
                }
            }
            Err(e) => OnlineValidationAttempt {
                was_network_attempt: true,
                error_message: Some(e.to_string()),
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Default)]
struct OnlineValidationAttempt {
    success: bool,
    was_network_attempt: bool,
    server_said_invalid: bool,
    error_message: Option<String>,
    record: Option<LicenseRecord>,
}

fn build_offline_stub_record(email: &str, token: &str) -> LicenseRecord {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expires_secs = now_secs + STUB_VALIDITY_DAYS * 86400;
    LicenseRecord {
        email: email.to_string(),
        token: token.to_string(),
        expires: format_iso8601(expires_secs),
        last_validated: format_iso8601(now_secs),
        validation_mode: "offline-stub".into(),
    }
}

fn is_license_fields_valid(record: &LicenseRecord) -> bool {
    !record.email.trim().is_empty()
        && !record.token.trim().is_empty()
        && !record.expires.trim().is_empty()
}

fn parse_rfc3339(s: &str) -> Option<SystemTime> {
    chrono_like_parse(s)
}

fn chrono_like_parse(s: &str) -> Option<SystemTime> {
    // Minimal ISO-8601 parse for license expiry (YYYY-MM-DDTHH:MM:SSZ or offset).
    let s = s.trim();
    if s.len() < 10 {
        return None;
    }
    let date: Vec<_> = s[..10].split('-').collect();
    if date.len() != 3 {
        return None;
    }
    let y: i64 = date[0].parse().ok()?;
    let m: i64 = date[1].parse().ok()?;
    let d: i64 = date[2].parse().ok()?;
    let days = unix_days_from_ymd(y, m, d)?;
    Some(UNIX_EPOCH + Duration::from_secs((days * 86400) as u64))
}

fn unix_days_from_ymd(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut yy = y;
    let mut mm = m;
    if mm <= 2 {
        yy -= 1;
        mm += 12;
    }
    let era = if yy >= 0 { yy / 400 } else { (yy - 399) / 400 };
    let yoe = yy - era * 400;
    let doy = (153 * (mm - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn format_iso8601(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let (y, m, d) = ymd_from_unix_days(days);
    format!("{y:04}-{m:02}-{d:02}T00:00:00Z")
}

fn ymd_from_unix_days(mut z: i64) -> (i64, i64, i64) {
    z += 719468;
    let era = if z >= 0 { z / 146097 } else { (z - 146096) / 146097 };
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

fn read_expires(root: &serde_json::Value) -> Option<String> {
    root.get("license")
        .and_then(|l| l.get("expires"))
        .and_then(|e| e.as_str())
        .map(str::to_string)
}

fn read_error_message(root: &serde_json::Value) -> Option<String> {
    root.get("error").and_then(|e| e.as_str()).map(str::to_string)
}

fn fail(message: impl Into<String>) -> LicenseActivationResult {
    LicenseActivationResult {
        success: false,
        message: message.into(),
        mode: LicenseValidationMode::None,
        expires: String::new(),
    }
}

pub fn is_pro_enabled() -> bool {
    if is_dev_bypass_enabled() {
        return true;
    }
    try_read_license(&license_file_path()).is_some()
}

pub fn status_label() -> String {
    if is_dev_bypass_enabled() {
        return "Pro (dev bypass: HYDROCOMPLETE_PRO=1)".into();
    }
    let path = license_file_path();
    if let Some(license) = try_read_license(&path) {
        if let Some(expires) = format_expiry_date(&license.expires) {
            return format!("Pro (licensed to {}, expires {expires})", license.email);
        }
        return format!("Pro (licensed to {})", license.email);
    }
    if let Some(expired) = try_read_license_metadata(&path) {
        if let Some(expires) = format_expiry_date(&expired.expires) {
            return format!("Expired (was {}, expired {expires})", expired.email);
        }
    }
    "Free".into()
}

pub fn validation_mode_label() -> String {
    if is_dev_bypass_enabled() {
        return "dev-bypass".into();
    }
    let path = license_file_path();
    let Some(license) = try_read_license_metadata(&path) else {
        return "none".into();
    };
    if license.validation_mode.trim().is_empty() {
        "local-file".into()
    } else {
        license.validation_mode
    }
}

pub fn last_validated_label() -> String {
    if is_dev_bypass_enabled() {
        return "n/a (dev bypass)".into();
    }
    let path = license_file_path();
    let Some(license) = try_read_license_metadata(&path) else {
        return "never".into();
    };
    if license.last_validated.trim().is_empty() {
        return "never".into();
    }
    format_expiry_date(&license.last_validated).unwrap_or(license.last_validated)
}

pub fn online_offline_label() -> String {
    if is_dev_bypass_enabled() {
        return "offline (environment bypass)".into();
    }
    match validation_mode_label().as_str() {
        "online" => "online (server validated)".into(),
        "offline-stub" => "offline (local beta stub)".into(),
        "none" => "offline (no license)".into(),
        _ => "offline (local file)".into(),
    }
}

fn format_expiry_date(iso: &str) -> Option<String> {
    if iso.len() < 10 {
        return None;
    }
    Some(iso[..10].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_formed_token() {
        assert!(is_well_formed_token("hc_live_abcdefgh"));
        assert!(!is_well_formed_token("bad_token"));
    }

    #[test]
    fn parse_combined_input() {
        let (e, t) = try_parse_combined_input("user@example.com hc_live_abcdefgh").unwrap();
        assert_eq!(e, "user@example.com");
        assert_eq!(t, "hc_live_abcdefgh");
    }
}