#[cfg(feature = "ssr")]
mod duo_driver;
#[cfg(feature = "ssr")]
pub use duo_driver::*;
#[cfg(feature = "ssr")]
mod duolingo;

pub mod ui;

use serde::{Deserialize, Serialize};

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(ui::App);
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Lesson {
    pub time: i64,
    pub xp: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyXPProgress {
    pub xp_goal: i64,
    pub lessons_today: Vec<Lesson>,
    pub xp_today: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SharedState {
    pub blocked: bool,
    pub xp_today: i64,
    pub xp_goal: i64,
    pub lessons: Vec<Lesson>,
    pub last_error: Option<String>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            blocked: true,
            xp_today: 0,
            xp_goal: 0,
            lessons: vec![],
            last_error: None,
        }
    }
}

// The actor commands. No separate events; actor updates SharedState directly.
pub enum ActorCommand {
    UpdateJWT(String),
    ForcePoll,
    Shutdown,
}
