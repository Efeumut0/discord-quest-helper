//! Rich Presence for the application itself.
//!
//! The card is published only while the app is open and no generated game is
//! running. A running game always owns the profile on its own, so this one
//! steps aside and comes back when the game closes.

use crate::rpc;
use chrono::{Local, Timelike};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Discord application whose name and art assets the card shows.
pub const PROMO_APP_ID: i64 = 1_575_534_676_205_117_570;
pub const DOWNLOAD_URL: &str = "https://github.com/Efeumut0/discord-quest-helper/releases/latest";
pub const LARGE_IMAGE_KEY: &str = "app_logo";

/// Discord truncates these server side; we keep within the documented limits.
const TEXT_LIMIT: usize = 128;
const BUTTON_LABEL_LIMIT: usize = 32;
/// Discord allows five activity updates per twenty seconds.
const UPDATE_INTERVAL: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Two quiet polls before the card returns, so it does not blink while a game
/// process is still shutting down.
const IDLE_POLLS_BEFORE_RESUME: u32 = 2;

pub(crate) static ACTIVITY_TRANSITION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub(crate) static PROMO_SESSION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

static ENABLED: AtomicBool = AtomicBool::new(false);
static LANGUAGE: Mutex<String> = Mutex::new(String::new());

#[derive(Clone, Serialize)]
pub struct PromoStatus {
    pub phase: String,
    pub error: Option<String>,
}

impl Default for PromoStatus {
    fn default() -> Self {
        Self {
            phase: "disabled".into(),
            error: None,
        }
    }
}

pub type State = Mutex<PromoStatus>;

/// The card contents, kept separate from the Discord builder so the text rules
/// can be tested without a Discord connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromoActivity {
    pub details: String,
    pub state: String,
    pub large_image: String,
    pub large_text: String,
    pub button_label: String,
    pub button_url: String,
    pub start_timestamp: i64,
}

impl PromoActivity {
    fn to_builder(&self) -> rpc::ds::activity::ActivityBuilder {
        rpc::ds::activity::ActivityBuilder::default()
            .kind(rpc::ds::activity::ActivityKind::Playing)
            .details(self.details.clone())
            .state(self.state.clone())
            .assets(
                rpc::ds::activity::Assets::default()
                    .large(self.large_image.clone(), Some(self.large_text.clone())),
            )
            .button(rpc::ds::activity::Button {
                label: self.button_label.clone(),
                url: self.button_url.clone(),
            })
            .start_timestamp(self.start_timestamp)
    }
}

pub fn clamp_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect()
}

/// Start of the local day for a moment described by its unix timestamp and the
/// seconds elapsed since local midnight. Discord counts up from this value, so
/// the card ends up showing the wall clock of the machine running the app.
pub fn day_start_timestamp(now_timestamp: i64, seconds_since_midnight: u32) -> i64 {
    now_timestamp - i64::from(seconds_since_midnight)
}

/// 5s, 15s, then 60s for every further attempt.
pub fn backoff_seconds(consecutive_failures: u32) -> u64 {
    match consecutive_failures {
        0 | 1 => 5,
        2 => 15,
        _ => 60,
    }
}

pub fn promo_activity(language: &str, start_timestamp: i64) -> PromoActivity {
    let turkish = language != "en";
    let details = if turkish {
        "Discord görevlerini tamamlıyor"
    } else {
        "Completing Discord quests"
    };
    let state = if turkish { "Boşta bekliyor" } else { "Idle" };
    let button_label = if turkish {
        "Programı indir"
    } else {
        "Download the app"
    };
    PromoActivity {
        details: clamp_text(details, TEXT_LIMIT),
        state: clamp_text(state, TEXT_LIMIT),
        large_image: LARGE_IMAGE_KEY.to_string(),
        large_text: clamp_text(
            &format!("Quest Helper v{}", env!("CARGO_PKG_VERSION")),
            TEXT_LIMIT,
        ),
        button_label: clamp_text(button_label, BUTTON_LABEL_LIMIT),
        button_url: DOWNLOAD_URL.to_string(),
        start_timestamp,
    }
}

fn current_language() -> String {
    LANGUAGE
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default()
}

fn current_activity() -> PromoActivity {
    let now = Local::now();
    let start = day_start_timestamp(now.timestamp(), now.num_seconds_from_midnight());
    promo_activity(&current_language(), start)
}

fn activity_takes_priority() -> bool {
    ACTIVITY_TRANSITION.try_lock().is_err()
        || crate::get_discord_client()
            .lock()
            .is_ok_and(|client| client.is_some())
        || generated_game_running()
}

fn generated_game_running() -> bool {
    crate::get_running_generated_game_processes()
        .map(|processes| !processes.is_empty())
        .unwrap_or(false)
}

fn report(app: &AppHandle, phase: &str, error: Option<String>) {
    let status = PromoStatus {
        phase: phase.into(),
        error,
    };
    if let Ok(mut state) = app.state::<State>().lock() {
        if state.phase == status.phase && state.error == status.error {
            return;
        }
        *state = status.clone();
    }
    let _ = app.emit("promo-presence-status", status);
}

#[tauri::command]
pub fn set_promo_presence(enabled: bool, language: String) -> Result<(), String> {
    if let Ok(mut value) = LANGUAGE.lock() {
        *value = language;
    }
    ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn get_promo_presence_status(state: tauri::State<'_, State>) -> Result<PromoStatus, String> {
    state
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "Promo presence status unavailable".to_string())
}

/// Holds one Discord connection open and keeps the card fresh. Returns once the
/// setting is turned off or a game takes over, and errors when Discord drops.
async fn session(app: &AppHandle) -> Result<(), String> {
    let _session = PROMO_SESSION.lock().await;
    if activity_takes_priority() {
        return Ok(());
    }
    let client = rpc::make_client(PROMO_APP_ID, rpc::ds::Subscriptions::ACTIVITY).await?;
    let mut last_sent: Option<Instant> = None;
    loop {
        if !ENABLED.load(Ordering::Relaxed) || activity_takes_priority() {
            let _ =
                tokio::time::timeout(Duration::from_secs(5), client.discord.clear_activity()).await;
            return Ok(());
        }
        if last_sent.is_none_or(|sent| sent.elapsed() >= UPDATE_INTERVAL) {
            let activity = current_activity();
            tokio::time::timeout(
                Duration::from_secs(10),
                client.discord.update_activity(activity.to_builder()),
            )
            .await
            .map_err(|_| "Discord activity update timed out".to_string())?
            .map_err(|error| error.to_string())?;
            last_sent = Some(Instant::now());
            // Acknowledgement proves submission, not profile visibility; the
            // viewer's Discord privacy settings still decide who sees it.
            report(app, "active", None);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn run_loop(app: AppHandle) {
    let mut consecutive_failures = 0u32;
    let mut quiet_polls = 0u32;
    loop {
        if !ENABLED.load(Ordering::Relaxed) {
            quiet_polls = 0;
            report(&app, "disabled", None);
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }
        if activity_takes_priority() {
            quiet_polls = 0;
            report(&app, "suspended", None);
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }
        quiet_polls += 1;
        if quiet_polls < IDLE_POLLS_BEFORE_RESUME {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }
        report(&app, "connecting", None);
        match session(&app).await {
            Ok(()) => consecutive_failures = 0,
            Err(error) => {
                eprintln!("Promo presence: {error}");
                consecutive_failures = consecutive_failures.saturating_add(1);
                report(&app, "unavailable", Some(error));
                tokio::time::sleep(Duration::from_secs(backoff_seconds(consecutive_failures)))
                    .await;
            }
        }
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move { run_loop(app).await });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_starts_at_local_midnight_so_the_card_shows_the_clock() {
        // 18:42:07 local time.
        let seconds_since_midnight = 18 * 3600 + 42 * 60 + 7;
        let now = 1_795_000_000;
        let start = day_start_timestamp(now, seconds_since_midnight);
        assert_eq!(now - start, i64::from(seconds_since_midnight));
        assert_eq!(start % 60, (now - i64::from(seconds_since_midnight)) % 60);
    }

    #[test]
    fn midnight_itself_starts_the_counter_at_zero() {
        let now = 1_795_000_000;
        assert_eq!(day_start_timestamp(now, 0), now);
    }

    #[test]
    fn text_stays_within_discord_limits() {
        let activity = promo_activity("tr", 0);
        assert!(activity.details.chars().count() <= TEXT_LIMIT);
        assert!(activity.state.chars().count() <= TEXT_LIMIT);
        assert!(activity.large_text.chars().count() <= TEXT_LIMIT);
        assert!(activity.button_label.chars().count() <= BUTTON_LABEL_LIMIT);
        assert_eq!(clamp_text("abcdef", 3), "abc");
        assert_eq!(clamp_text("ğüşiöç", 3), "ğüş");
        assert_eq!(clamp_text("kısa", 99), "kısa");
    }

    #[test]
    fn card_points_at_the_release_page_and_the_uploaded_art_asset() {
        let activity = promo_activity("tr", 1234);
        assert_eq!(activity.button_url, DOWNLOAD_URL);
        assert!(activity.button_url.ends_with("/releases/latest"));
        assert_eq!(activity.large_image, "app_logo");
        assert_eq!(activity.start_timestamp, 1234);
        assert_eq!(activity.details, "Discord görevlerini tamamlıyor");
        assert_eq!(activity.state, "Boşta bekliyor");
    }

    #[test]
    fn english_is_used_only_for_the_english_interface() {
        assert_eq!(promo_activity("en", 0).state, "Idle");
        for language in ["tr", "", "de"] {
            assert_eq!(promo_activity(language, 0).state, "Boşta bekliyor");
        }
    }

    #[test]
    fn retries_slow_down_while_discord_stays_closed() {
        assert_eq!(backoff_seconds(1), 5);
        assert_eq!(backoff_seconds(2), 15);
        assert_eq!(backoff_seconds(3), 60);
        assert_eq!(backoff_seconds(40), 60);
    }

    #[test]
    fn presence_is_off_until_the_interface_asks_for_it() {
        ENABLED.store(false, Ordering::Relaxed);
        assert!(!ENABLED.load(Ordering::Relaxed));
        set_promo_presence(true, "tr".into()).unwrap();
        assert!(ENABLED.load(Ordering::Relaxed));
        assert_eq!(current_language(), "tr");
        set_promo_presence(false, "en".into()).unwrap();
        assert!(!ENABLED.load(Ordering::Relaxed));
        assert_eq!(current_language(), "en");
    }
}
