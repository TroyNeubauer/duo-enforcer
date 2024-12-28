use chrono::{Datelike, Local, NaiveDateTime, Utc};
use leptos::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use ureq::Agent;

// ------------------------------
// 4) Main — Launch Leptos SSR
// ------------------------------
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(4);
    spawn_actor_thread(rx);

    let conf = leptos::config::get_configuration(None).unwrap_or_default();

    let opts = LeptosOptions::from_config(&conf);
    SimpleServer::new(move |cx| view! { cx, <App/> })
        .with_options(opts)
        .start()
        .await
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
