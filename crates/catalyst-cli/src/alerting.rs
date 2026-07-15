//! Best-effort Discord webhook alerting for consensus circuit-breaker trips.
//!
//! Alerting must never affect consensus correctness or liveness: [`notify`] is fire-and-forget
//! (spawns a task, never awaited by the caller) and any failure is logged, never propagated.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tracing::{info, warn};

/// Discord (or any Discord-webhook-compatible) endpoint to POST alerts to
/// (`CATALYST_ALERT_WEBHOOK_URL`). Unset/empty disables alerting entirely -- every [`notify`] call
/// becomes a no-op.
pub fn alert_webhook_url_from_env() -> Option<String> {
    std::env::var("CATALYST_ALERT_WEBHOOK_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Minimum gap (seconds) between repeat alerts for the same `kind`, so a node stuck in the breaker
/// for hours doesn't spam the channel after the first page has gone out
/// (`CATALYST_ALERT_COOLDOWN_SECS`, default `900` = 15 min, max `86400` = 24h).
pub fn alert_cooldown_secs_from_env() -> u64 {
    const MAX: u64 = 86_400;
    const DEFAULT: u64 = 900;
    std::env::var("CATALYST_ALERT_COOLDOWN_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&v| v > 0 && v <= MAX)
        .unwrap_or(DEFAULT)
}

struct AlertSender {
    client: reqwest::Client,
    webhook_url: String,
}

fn alert_sender() -> Option<&'static AlertSender> {
    static SENDER: OnceLock<Option<AlertSender>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            alert_webhook_url_from_env().map(|webhook_url| AlertSender {
                client: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new()),
                webhook_url,
            })
        })
        .as_ref()
}

fn last_sent_ms_map() -> &'static Mutex<HashMap<String, u64>> {
    static MAP: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Pure cooldown decision: has enough time passed since the last alert of this `kind`? Records
/// `now_ms` as the new last-sent time when it returns `true`, so callers don't need a separate
/// "mark as sent" step.
fn should_send(kind: &str, now_ms: u64, map: &mut HashMap<String, u64>, cooldown_ms: u64) -> bool {
    match map.get(kind) {
        Some(&last_ms) if now_ms.saturating_sub(last_ms) < cooldown_ms => false,
        _ => {
            map.insert(kind.to_string(), now_ms);
            true
        }
    }
}

/// Fire-and-forget best-effort Discord alert. No-op if `CATALYST_ALERT_WEBHOOK_URL` is unset, or if
/// an alert of the same `kind` was already sent within the configured cooldown window. Never blocks
/// or errors the caller -- consensus liveness must never depend on this succeeding.
pub fn notify(kind: &str, message: String) {
    let Some(sender) = alert_sender() else {
        return;
    };
    let now_ms = catalyst_utils::utils::current_timestamp_ms();
    let cooldown_ms = alert_cooldown_secs_from_env().saturating_mul(1000);
    {
        let mut map = match last_sent_ms_map().lock() {
            Ok(m) => m,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !should_send(kind, now_ms, &mut map, cooldown_ms) {
            return;
        }
    }

    let client = sender.client.clone();
    let webhook_url = sender.webhook_url.clone();
    tokio::spawn(async move {
        let body = serde_json::json!({ "content": message });
        if let Err(e) = client.post(&webhook_url).json(&body).send().await {
            warn!(target: "catalyst.alerting", "Discord alert POST failed: {e}");
        }
    });
}

/// Logged once at startup so it's obvious from the log whether a circuit-breaker trip will
/// actually page anyone.
pub fn log_alerting_status() {
    match alert_webhook_url_from_env() {
        Some(_) => info!(
            target: "catalyst.alerting",
            cooldown_secs = alert_cooldown_secs_from_env(),
            "Alerting configured: consensus circuit-breaker trips will post to the configured Discord webhook"
        ),
        None => warn!(
            target: "catalyst.alerting",
            "CATALYST_ALERT_WEBHOOK_URL not set -- consensus circuit-breaker trips will only be visible via logs/metrics, nobody will be paged"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }

        fn set(key: &'static str, val: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, val);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(ref v) = self.previous {
                std::env::set_var(self.key, v);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn alert_webhook_url_from_env_trims_and_treats_empty_as_unset() {
        let _e1 = EnvGuard::unset("CATALYST_ALERT_WEBHOOK_URL");
        assert_eq!(alert_webhook_url_from_env(), None);

        let _e2 = EnvGuard::set("CATALYST_ALERT_WEBHOOK_URL", "  ");
        assert_eq!(alert_webhook_url_from_env(), None);

        let _e3 = EnvGuard::set(
            "CATALYST_ALERT_WEBHOOK_URL",
            " https://discord.com/api/webhooks/x/y \n",
        );
        assert_eq!(
            alert_webhook_url_from_env(),
            Some("https://discord.com/api/webhooks/x/y".to_string())
        );
    }

    #[test]
    fn alert_cooldown_secs_from_env_default_and_bounds() {
        let _e = EnvGuard::unset("CATALYST_ALERT_COOLDOWN_SECS");
        assert_eq!(alert_cooldown_secs_from_env(), 900);

        let _e2 = EnvGuard::set("CATALYST_ALERT_COOLDOWN_SECS", "60");
        assert_eq!(alert_cooldown_secs_from_env(), 60);

        // Out of range (0, or above the 24h max) falls back to the default rather than disabling
        // the cooldown or accepting an unreasonable value.
        let _e3 = EnvGuard::set("CATALYST_ALERT_COOLDOWN_SECS", "0");
        assert_eq!(alert_cooldown_secs_from_env(), 900);
        let _e4 = EnvGuard::set("CATALYST_ALERT_COOLDOWN_SECS", "999999999");
        assert_eq!(alert_cooldown_secs_from_env(), 900);
    }

    #[test]
    fn should_send_respects_cooldown_and_is_per_kind() {
        let mut map = HashMap::new();
        let cooldown_ms = 10_000;

        assert!(should_send("a", 1_000, &mut map, cooldown_ms));
        // Same kind, within cooldown -> suppressed.
        assert!(!should_send("a", 5_000, &mut map, cooldown_ms));
        // Different kind is tracked independently of "a"'s cooldown.
        assert!(should_send("b", 5_000, &mut map, cooldown_ms));
        // Same kind, cooldown elapsed -> sends again.
        assert!(should_send("a", 11_001, &mut map, cooldown_ms));
    }
}
