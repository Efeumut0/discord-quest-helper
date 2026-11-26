//! Optional profile appearance for Source 2. Source 1 never enables this client.
use discord_sdk::{
    activity::ActivityBuilder, wheel::UserState, Discord, DiscordApp, Subscriptions,
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static STATE: AtomicU8 = AtomicU8::new(0);

pub fn parse_client_id(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<i64>().ok().filter(|id| *id > 0)
}

pub fn status_text() -> &'static str {
    match STATE.load(Ordering::Relaxed) {
        1 => crate::loc("Discord RPC: connecting", "Discord RPC: bağlanıyor"),
        2 => crate::loc(
            "Discord RPC: activity accepted",
            "Discord RPC: etkinlik gönderildi",
        ),
        3 => crate::loc(
            "Discord RPC unavailable; retrying",
            "Discord RPC kurulamadı; tekrar deneniyor",
        ),
        _ => crate::loc("Game process is running", "Oyun işlemi çalışıyor"),
    }
}

pub fn start(embedded_client_id: Option<String>) {
    let Some(value) = embedded_client_id else {
        return;
    };
    let Some(app_id) = parse_client_id(&value) else {
        STATE.store(3, Ordering::Relaxed);
        return;
    };
    STATE.store(1, Ordering::Relaxed);
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("RPC runtime: {error}");
                STATE.store(3, Ordering::Relaxed);
                return;
            }
        };
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        runtime.block_on(async move {
            loop {
                if let Err(error) = session(app_id, started_at, false).await {
                    STATE.store(3, Ordering::Relaxed);
                    eprintln!("Discord RPC: {error}");
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                STATE.store(1, Ordering::Relaxed);
            }
        });
    });
}

async fn session(app_id: i64, started_at: i64, one_shot: bool) -> Result<(), String> {
    let (wheel, handler) = discord_sdk::wheel::Wheel::new(Box::new(|error| {
        eprintln!("Discord RPC transport: {error}");
    }));
    let mut user = wheel.user();
    let discord = Discord::new(
        DiscordApp::PlainId(app_id),
        Subscriptions::ACTIVITY,
        Box::new(handler),
    )
    .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(15), user.0.changed())
        .await
        .map_err(|_| "Discord connection timed out".to_string())?
        .map_err(|error| error.to_string())?;
    if !matches!(&*user.0.borrow(), UserState::Connected(_)) {
        return Err("Discord desktop is not connected".to_string());
    }
    loop {
        let activity = ActivityBuilder::default()
            .kind(discord_sdk::activity::ActivityKind::Playing)
            .start_timestamp(started_at);
        tokio::time::timeout(Duration::from_secs(10), discord.update_activity(activity))
            .await
            .map_err(|_| "Discord activity update timed out".to_string())?
            .map_err(|error| error.to_string())?;
        // A successful acknowledgement proves submission, not profile visibility
        // or quest eligibility; Discord privacy and game rules still apply.
        STATE.store(2, Ordering::Relaxed);
        if one_shot {
            tokio::time::timeout(Duration::from_secs(5), discord.clear_activity())
                .await
                .map_err(|_| "Discord activity cleanup timed out".to_string())?
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_catalog_application_ids_and_rejects_invalid_ids() {
        assert_eq!(
            parse_client_id("1445160676522856458"),
            Some(1445160676522856458)
        );
        for value in ["", "0", "-1", "+123", "not-an-id", "18446744073709551615"] {
            assert_eq!(parse_client_id(value), None, "{value}");
        }
    }

    #[test]
    fn ordinary_runner_does_not_enable_rpc_without_explicit_resource() {
        STATE.store(0, Ordering::Relaxed);
        start(None);
        assert_eq!(STATE.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    #[ignore = "manual test against the locally running Discord desktop"]
    async fn live_discord_accepts_and_clears_activity() {
        let value = std::env::var("DQH_RPC_SMOKE_APP_ID").expect("Provide catalog application ID");
        let app_id = parse_client_id(&value).expect("Valid ID");
        session(
            app_id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            true,
        )
        .await
        .expect("Discord must acknowledge SET_ACTIVITY and clear it");
    }
}
