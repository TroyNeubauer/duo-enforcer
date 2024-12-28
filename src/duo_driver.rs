use chrono::{Datelike, Local, NaiveDateTime, Utc};
use crossbeam_channel::Receiver;
use crossbeam_channel::RecvTimeoutError;
use leptos::*;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::sync::OnceLock;
use std::time::Instant;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::{duolingo::Duolingo, ActorCommand, SharedState};

#[cfg(feature = "ssr")]
pub(crate) static SHARED_STATE: LazyLock<Arc<Mutex<SharedState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(SharedState::default())));
#[cfg(feature = "ssr")]
pub(crate) static CMD_TX: OnceLock<crossbeam_channel::Sender<ActorCommand>> = OnceLock::new();

// The actor thread: polls Duolingo every 5 minutes or on command
fn spawn_actor_thread(rx: Receiver<ActorCommand>) {
    // read initial JWT from env
    let initial_jwt = std::env::var("DUO_JWT").unwrap_or_else(|_| "".to_string());
    let xp_req = 50; // daily XP requirement
    let done_file = dirs::home_dir()
        .unwrap_or_else(|| "/tmp".into())
        .join(".cache/duo-done");

    // Try to init Duolingo
    let mut duo = match Duolingo::new(&initial_jwt) {
        Ok(d) => Some(d),
        Err(e) => {
            let mut st = SHARED_STATE.lock().unwrap();
            st.last_error = Some(format!("init error: {e}"));
            None
        }
    };

    let mut next_poll = Instant::now();
    loop {
        match rx.recv_deadline(next_poll) {
            Ok(cmd) => match cmd {
                ActorCommand::UpdateJWT(new_jwt) => match Duolingo::new(&new_jwt) {
                    Ok(d) => {
                        duo = Some(d);
                        let mut st = SHARED_STATE.lock().unwrap();
                        st.last_error = Some("JWT updated OK".to_string());
                    }
                    Err(e) => {
                        let mut st = SHARED_STATE.lock().unwrap();
                        st.last_error = Some(format!("JWT update failed: {e}"));
                    }
                },
                ActorCommand::ForcePoll => poll_duo(&mut duo, xp_req, &done_file),
                ActorCommand::Shutdown => {
                    // optional graceful exit
                    return;
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                poll_duo(&mut duo, xp_req, &done_file);
            }
            Err(RecvTimeoutError::Disconnected) => {
                // main sender is gone => just exit
                return;
            }
        }
        next_poll += Duration::from_secs(30);
    }
}

fn poll_duo(duo: &mut Option<Duolingo>, xp_req: i64, done_file: &PathBuf) {
    if let Some(d) = duo {
        match d.get_daily_xp_progress() {
            Ok(prog) => {
                let mut st = SHARED_STATE.lock().unwrap();
                st.xp_goal = prog.xp_goal;
                st.xp_today = prog.xp_today;
                st.lessons = prog.lessons_today;
                st.last_error = None;
                // blocked?
                st.blocked = prog.xp_today < xp_req;
                if prog.xp_today >= xp_req {
                    // write done file
                    let today_str = Local::now().format("%Y-%m-%d").to_string();
                    let _ = fs::create_dir_all(done_file.parent().unwrap());
                    if let Err(e) = fs::write(done_file, &today_str) {
                        st.last_error = Some(format!("Failed to write done file: {e}"));
                    }
                }
            }
            Err(e) => {
                let mut st = SHARED_STATE.lock().unwrap();
                st.last_error = Some(format!("Poll error: {e}"));
            }
        }
    } else {
        let mut st = SHARED_STATE.lock().unwrap();
        st.last_error = Some("No duolingo client available (JWT init failed)".to_string());
    }
}
