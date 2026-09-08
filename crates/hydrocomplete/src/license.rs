//! Pro license activation for the Open CAD Studio SKU (`product = "opencad"`).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const DEFAULT_VALIDATE_URL: &str =
    "https://hc-refactored.fly.dev/api/licensing/validate";

pub const TOKEN_PREFIX: &str = "hc_live_";

/// In-product activation command, for status messages.
const ACTIVATE_CMD: &str = "HC_ACTIVATE";

/// Server-side SKU identifier for this client (separate from Civil 3D `civil3d` keys).
pub const PRODUCT_ID: &str = "opencad";

pub const PRODUCT_LABEL: &str = "Open CAD Studio";

pub const PURCHASE_URL: &str = "https://hydrocomplete.com/opencad";

pub const LICENSE_FILE_NAME: &str = "opencad-license.json";

pub const STUB_VALIDITY_DAYS: u64 = 365;

/// Re-check the key with the server once the cached validation is this old.
pub const REVALIDATE_AFTER_DAYS: u64 = 7;
/// If the server cannot be reached, keep honouring the cached license for this
/// long after the last successful validation, then require re-activation.
pub const OFFLINE_GRACE_DAYS: u64 = 30;
/// Do not hit the network for a re-check more than once per day.
pub const RECHECK_THROTTLE_DAYS: u64 = 1;
/// Short timeouts for the silent background re-check so a gated command is not
/// held up for long when offline (activation itself uses longer timeouts).
const RECHECK_TIMEOUT: Duration = Duration::from_secs(4);

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
    pub product: String,
    #[serde(default)]
    pub last_validated: String,
    #[serde(default, rename = "validationMode")]
    pub validation_mode: String,
    /// Last time a silent re-check was attempted (success or not); throttles
    /// network attempts while offline.
    #[serde(default, rename = "lastCheckAttempt")]
    pub last_check_attempt: String,
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
        base.join("HydroComplete").join(LICENSE_FILE_NAME)
    } else {
        PathBuf::from(format!(".{LICENSE_FILE_NAME}"))
    }
}

pub fn is_dev_bypass_enabled() -> bool {
    #[cfg(not(debug_assertions))]
    {
        false
    }
    #[cfg(debug_assertions)]
    {
        std::env::var("HYDROCOMPLETE_PRO")
            .map(|v| v == "1")
            .unwrap_or(false)
    }
}

/// The `hc_live_*` key behind a stored token. Legacy license files (before the
/// re-validation release) stored the server's base64 access token, whose
/// payload is `{"licenseKey": ...}` + a signature after the last dot.
pub fn underlying_license_key(token: &str) -> String {
    let t = token.trim();
    if t.starts_with(TOKEN_PREFIX) {
        return t.to_string();
    }
    let decoded = base64_decode_loose(t);
    let Some(decoded) = decoded else {
        return t.to_string();
    };
    let payload = match decoded.rfind('.') {
        Some(i) => &decoded[..i],
        None => decoded.as_str(),
    };
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get("licenseKey").and_then(|k| k.as_str()).map(str::to_string))
        .filter(|k| k.starts_with(TOKEN_PREFIX))
        .unwrap_or_else(|| t.to_string())
}

/// Minimal standard-alphabet base64 decoder (no padding required) so the
/// plugin does not need another dependency for one legacy path.
fn base64_decode_loose(s: &str) -> Option<String> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = val(c)?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    String::from_utf8(out).ok()
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
    timeout: Duration,
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
            timeout: Duration::from_secs(15),
        }
    }

    pub fn with_validate_url(mut self, url: impl Into<String>) -> Self {
        self.validate_url = url.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
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
        if existing.product != PRODUCT_ID {
            return fail(wrong_product_message());
        }
        let key = underlying_license_key(&existing.token);
        self.activate_core(&existing.email, &key, license_path)
    }

    fn activate_core(&self, email: &str, token: &str, license_path: &Path) -> LicenseActivationResult {
        let online = self.try_online_validation(email, token);
        if online.success {
            if let Some(record) = online.record {
                let _ = write_license_file(license_path, &record);
                return LicenseActivationResult {
                    success: true,
                    message: format!("Pro activated for {PRODUCT_LABEL} (online validation)."),
                    mode: LicenseValidationMode::Online,
                    expires: record.expires,
                };
            }
        }
        if online.server_said_invalid {
            let detail = online
                .error_message
                .unwrap_or_else(|| "License is not valid on the server.".into());
            return fail(format!("{detail} {}", wrong_product_hint()));
        }
        if !is_well_formed_token(token) {
            return fail(
                online
                    .error_message
                    .unwrap_or_else(|| "Online validation failed and token format is invalid.".into()),
            );
        }

        #[cfg(not(debug_assertions))]
        {
            let detail = online.error_message.unwrap_or_else(|| {
                "Could not reach the license server.".into()
            });
            return fail(format!(
                "{detail} Purchase an Open CAD Studio key at {PURCHASE_URL} and try again."
            ));
        }

        #[cfg(debug_assertions)]
        {
            let stub = build_offline_stub_record(email, token);
            let _ = write_license_file(license_path, &stub);
            let message = if online.was_network_attempt {
                format!(
                    "Pro activated (offline stub — server unreachable; dev build only). Purchase: {PURCHASE_URL}"
                )
            } else {
                "Pro activated (offline stub — dev build only).".into()
            };
            LicenseActivationResult {
                success: true,
                message,
                mode: LicenseValidationMode::OfflineStub,
                expires: stub.expires.clone(),
            }
        }
    }

    fn try_online_validation(&self, email: &str, token: &str) -> OnlineValidationAttempt {
        let token = underlying_license_key(token);
        let token = token.as_str();
        let body = serde_json::json!({
            "licenseKey": token,
            "product": PRODUCT_ID,
            "features": ["reports", "export"],
        });
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(self.timeout)
            .timeout_read(self.timeout)
            .user_agent(&user_agent_string())
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
                // Store the license key itself. Earlier builds stored the
                // server's accessToken here, which cannot be re-validated;
                // `underlying_license_key` unwraps those legacy files.
                let stored_token = token.to_string();
                let now = format_iso8601(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                );
                OnlineValidationAttempt {
                    success: true,
                    was_network_attempt: true,
                    record: Some(new_license_record(
                        email,
                        &stored_token,
                        &expires,
                        &now,
                        "online",
                    )),
                    ..Default::default()
                }
            }
            // ureq reports 4xx/5xx as Err. A 401/403 (or an explicit
            // `valid:false` body) is the server rejecting the key — revoked,
            // expired or wrong product — not a network failure, and must not
            // fall into the offline-grace path.
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
                let explicit_invalid = parsed
                    .as_ref()
                    .and_then(|v| v.get("valid"))
                    .and_then(|x| x.as_bool())
                    == Some(false);
                let rejected = matches!(code, 401 | 403) || explicit_invalid;
                let detail = parsed
                    .as_ref()
                    .and_then(read_error_message)
                    .unwrap_or_else(|| format!("Server returned {code}."));
                OnlineValidationAttempt {
                    was_network_attempt: true,
                    server_said_invalid: rejected,
                    error_message: Some(detail),
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

fn new_license_record(
    email: &str,
    token: &str,
    expires: &str,
    last_validated: &str,
    validation_mode: &str,
) -> LicenseRecord {
    LicenseRecord {
        email: email.to_string(),
        token: token.to_string(),
        expires: expires.to_string(),
        product: PRODUCT_ID.into(),
        last_validated: last_validated.to_string(),
        validation_mode: validation_mode.into(),
        last_check_attempt: String::new(),
    }
}

fn build_offline_stub_record(email: &str, token: &str) -> LicenseRecord {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expires_secs = now_secs + STUB_VALIDITY_DAYS * 86400;
    new_license_record(
        email,
        token,
        &format_iso8601(expires_secs),
        &format_iso8601(now_secs),
        "offline-stub",
    )
}

fn is_license_fields_valid(record: &LicenseRecord) -> bool {
    !record.email.trim().is_empty()
        && !record.token.trim().is_empty()
        && !record.expires.trim().is_empty()
        && record.product == PRODUCT_ID
}

fn wrong_product_message() -> String {
    format!("This license file is not for {PRODUCT_LABEL} (product={PRODUCT_ID}).")
}

fn wrong_product_hint() -> String {
    format!("Open CAD Studio keys are sold at {PURCHASE_URL}. Civil 3D keys use a separate SKU.")
}

fn user_agent_string() -> String {
    format!("HydroComplete-OpenCAD/{}", env!("CARGO_PKG_VERSION"))
}

fn parse_rfc3339(s: &str) -> Option<SystemTime> {
    chrono_like_parse(s)
}

fn chrono_like_parse(s: &str) -> Option<SystemTime> {
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
    let path = license_file_path();
    let Some(record) = try_read_license(&path) else {
        return false;
    };
    matches!(revalidate_if_stale(&path, record), Revalidation::Current | Revalidation::OfflineGrace)
}

/// Outcome of the silent periodic re-check performed by [`is_pro_enabled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revalidation {
    /// Validated recently, or re-validated online just now.
    Current,
    /// Server unreachable; cached license still inside the offline grace window.
    OfflineGrace,
    /// Server unreachable and the grace window has expired; re-activation required.
    GraceExpired,
    /// Server explicitly rejected the key (revoked / expired); license file removed.
    Revoked,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn age_days(iso: &str) -> Option<u64> {
    let then = parse_rfc3339(iso)?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(now_secs().saturating_sub(then) / 86400)
}

/// Re-check a locally valid license against the server when its last
/// validation is older than [`REVALIDATE_AFTER_DAYS`]. Network failures fall
/// back to the cached license for [`OFFLINE_GRACE_DAYS`]; explicit rejection
/// deletes the cached file so a revoked key stops working within a week.
pub fn revalidate_if_stale(path: &Path, record: LicenseRecord) -> Revalidation {
    // Stub / bypass records never came from the server; leave them alone.
    if record.validation_mode != "online" {
        return Revalidation::Current;
    }
    let validated_days = age_days(&record.last_validated).unwrap_or(u64::MAX);
    if validated_days < REVALIDATE_AFTER_DAYS {
        return Revalidation::Current;
    }
    let attempted_days = age_days(&record.last_check_attempt).unwrap_or(u64::MAX);
    let past_grace = validated_days >= OFFLINE_GRACE_DAYS;
    if attempted_days < RECHECK_THROTTLE_DAYS {
        // Tried recently and could not reach the server; do not retry yet.
        return if past_grace { Revalidation::GraceExpired } else { Revalidation::OfflineGrace };
    }

    let activator = LicenseActivator::new().with_timeout(RECHECK_TIMEOUT);
    let attempt = activator.try_online_validation(&record.email, &record.token);
    if attempt.success {
        if let Some(fresh) = attempt.record {
            let _ = write_license_file(path, &fresh);
        }
        return Revalidation::Current;
    }
    if attempt.server_said_invalid {
        let _ = std::fs::remove_file(path);
        return Revalidation::Revoked;
    }
    // Unreachable / bad response: stamp the attempt so we stay quiet for a day.
    let mut stamped = record;
    stamped.last_check_attempt = format_iso8601(now_secs());
    let _ = write_license_file(path, &stamped);
    if past_grace { Revalidation::GraceExpired } else { Revalidation::OfflineGrace }
}

pub fn status_label() -> String {
    if is_dev_bypass_enabled() {
        return format!("Pro ({PRODUCT_LABEL}, dev bypass: HYDROCOMPLETE_PRO=1)");
    }
    let path = license_file_path();
    if let Some(license) = try_read_license(&path) {
        let validated_days = age_days(&license.last_validated).unwrap_or(0);
        if license.validation_mode == "online" && validated_days >= OFFLINE_GRACE_DAYS {
            return format!(
                "Pro ({PRODUCT_LABEL}, licensed to {}) — NOT re-validated in {validated_days} days; connect and run {ACTIVATE_CMD} to keep Pro features",
                license.email
            );
        }
        if let Some(expires) = format_expiry_date(&license.expires) {
            return format!(
                "Pro ({PRODUCT_LABEL}, licensed to {}, expires {expires}, last validated {} days ago)",
                license.email, validated_days
            );
        }
        return format!(
            "Pro ({PRODUCT_LABEL}, licensed to {})",
            license.email
        );
    }
    if let Some(stored) = try_read_license_metadata(&path) {
        if !stored.product.is_empty() && stored.product != PRODUCT_ID {
            return format!(
                "Free (license file is for product '{}', not {PRODUCT_ID})",
                stored.product
            );
        }
        if let Some(expires) = format_expiry_date(&stored.expires) {
            return format!("Expired ({PRODUCT_LABEL}, was {}, expired {expires})", stored.email);
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
        "offline-stub" => "offline (local beta stub, dev build only)".into(),
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

    fn record_validated_days_ago(days: u64) -> LicenseRecord {
        let now = now_secs();
        let mut r = new_license_record(
            "user@example.com",
            "hc_live_abcdefgh",
            &format_iso8601(now + 365 * 86400),
            &format_iso8601(now.saturating_sub(days * 86400)),
            "online",
        );
        r.last_check_attempt = format_iso8601(now); // throttled: no network in tests
        r
    }

    #[test]
    fn fresh_license_is_current_without_network() {
        let dir = std::env::temp_dir().join(format!("lic-fresh-{}", std::process::id()));
        let path = dir.join(LICENSE_FILE_NAME);
        assert_eq!(revalidate_if_stale(&path, record_validated_days_ago(1)), Revalidation::Current);
    }

    #[test]
    fn stale_but_throttled_license_keeps_offline_grace() {
        let dir = std::env::temp_dir().join(format!("lic-stale-{}", std::process::id()));
        let path = dir.join(LICENSE_FILE_NAME);
        assert_eq!(
            revalidate_if_stale(&path, record_validated_days_ago(REVALIDATE_AFTER_DAYS + 1)),
            Revalidation::OfflineGrace
        );
    }

    #[test]
    fn grace_expires_after_offline_window() {
        let dir = std::env::temp_dir().join(format!("lic-grace-{}", std::process::id()));
        let path = dir.join(LICENSE_FILE_NAME);
        assert_eq!(
            revalidate_if_stale(&path, record_validated_days_ago(OFFLINE_GRACE_DAYS + 1)),
            Revalidation::GraceExpired
        );
    }

    #[test]
    fn offline_stub_records_are_never_rechecked() {
        let dir = std::env::temp_dir().join(format!("lic-stub-{}", std::process::id()));
        let path = dir.join(LICENSE_FILE_NAME);
        let mut r = record_validated_days_ago(400);
        r.validation_mode = "offline-stub".into();
        assert_eq!(revalidate_if_stale(&path, r), Revalidation::Current);
    }

    #[test]
    fn legacy_access_token_unwraps_to_license_key() {
        // base64("{\"licenseKey\":\"hc_live_ss_abc123\",\"type\":\"x\"}.sig")
        let payload = "{\"licenseKey\":\"hc_live_ss_abc123\",\"type\":\"x\"}.deadbeef";
        let enc = {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let b = payload.as_bytes();
            let mut o = String::new();
            for ch in b.chunks(3) {
                let n = (ch[0] as u32) << 16
                    | (*ch.get(1).unwrap_or(&0) as u32) << 8
                    | *ch.get(2).unwrap_or(&0) as u32;
                o.push(T[(n >> 18) as usize & 63] as char);
                o.push(T[(n >> 12) as usize & 63] as char);
                o.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
                o.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
            }
            o
        };
        assert_eq!(underlying_license_key(&enc), "hc_live_ss_abc123");
        assert_eq!(underlying_license_key("hc_live_plain00"), "hc_live_plain00");
        assert_eq!(underlying_license_key("garbage"), "garbage");
    }

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

    #[test]
    fn accepts_opencad_product_only() {
        let record = new_license_record(
            "user@example.com",
            "hc_live_abcdefgh",
            "2099-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            "online",
        );
        assert!(is_license_fields_valid(&record));
        assert_eq!(record.product, PRODUCT_ID);
    }

    #[test]
    fn rejects_civil3d_product() {
        let mut record = new_license_record(
            "user@example.com",
            "hc_live_abcdefgh",
            "2099-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            "online",
        );
        record.product = "civil3d".into();
        assert!(!is_license_fields_valid(&record));
    }

    #[test]
    fn license_file_uses_opencad_name() {
        let path = license_file_path();
        assert!(path.to_string_lossy().ends_with(LICENSE_FILE_NAME));
    }
}