mod duolingo;
pub use duolingo::*;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::{signal, sync::Mutex};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

#[derive(Clone)]
struct AppState {
    duo: Arc<Mutex<DuolingoApi>>,
}

/// Body for `POST /api/update_jwt`
#[derive(Deserialize)]
struct UpdateJwtBody {
    new_jwt: String,
}

const DAILY_XP_GOAL: u32 = 100;

const PERSISTENT_JWT_STORAGE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut home = dirs::home_dir().expect("Failed to get home directory");
    home.push(".duo_jwt_token");
    home
});

mod enforcer {
    use std::io::ErrorKind;

    use super::*;

    const IS_FINISHED_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let mut home = dirs::home_dir().expect("Failed to get home directory");
        home.push(".duo_done");
        home
    });

    pub fn is_disarmed() -> bool {
        std::fs::exists(&*IS_FINISHED_PATH).unwrap_or(false)
    }

    pub fn block_all() {
        let path = &*IS_FINISHED_PATH;
        if is_disarmed() {
            info!("Block enabled. Removing path {path:?}");
        }
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != ErrorKind::NotFound {
                warn!("Failed to remove path {path:?}: {e}");
            }
        }
    }

    pub fn disarm() {
        let path = &*IS_FINISHED_PATH;
        if !is_disarmed() {
            info!("Block disabled. Creating path {path:?}");
        }
        if let Err(e) = std::fs::write(path, "") {
            warn!("Failed to create path {path:?}: {e}");
        }
    }
}

/// GET / => returns a minimal HTML/JS front end
async fn ui_handler() -> impl IntoResponse {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8"/>
  <title>Duolingo Enforcer UI</title>
  <style>
    body { font-family: sans-serif; margin: 1em; }
    .error { color: red; }
    .blocked { color: red; font-size: 20px; }
    .unblocked { color: green; font-size: 20px; }
  </style>
</head>
<body>
  <h1>Duolingo Enforcer UI</h1>
  <div>
    <button id="refreshBtn">Refresh Status</button>
    <div style="margin-top:1em;">
      <input id="jwtBox" placeholder="New JWT..." style="width:300px;" />
      <button id="jwtBtn">Update JWT</button>
    </div>
  </div>
  
  <hr/>
  <div id="statusArea">Loading status...</div>
  <div id="errorArea"></div>

  <script>
    let jwtInput = "";

    async function fetchStatus() {
      clearError();
      setStatus("Loading status...");
      try {
        const resp = await fetch("/api/status");
        if (!resp.ok) {
          const text = await resp.text();
          throw new Error(text);
        }
        const data = await resp.json();
        renderStatus(data);
      } catch(e) {
        showError(e);
      }
    }

    async function updateJwt() {
      clearError();
      try {
        const resp = await fetch("/api/update_jwt", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ new_jwt: jwtInput }),
        });
        if (!resp.ok) {
          const text = await resp.text();
          throw new Error(text);
        }
        // after updating, refresh
        fetchStatus();
      } catch(e) {
        showError(e);
      }
    }

    function renderStatus(data) {
      const statusArea = document.getElementById("statusArea");
      const { xp_goal, xp_today, lessons_today, blocked } = data;
      // blocked => red, else green
      const colorClass = blocked ? "blocked" : "unblocked";
      const text = blocked 
        ? `BLOCKED! (XP: ${xp_today}/${xp_goal})`
        : `UNBLOCKED! (XP: ${xp_today}/${xp_goal})`;
      
      let html = `<div class="${colorClass}">${text}</div>`;
      if (lessons_today && lessons_today.length > 0) {
        html += `<h3>Recent Lessons</h3>`;
        // group by day
        const groupMap = {};
        for (const ls of lessons_today) {
          const dt = new Date(ls.time * 1000);
          const dayStr = dt.toLocaleDateString();
          if (!groupMap[dayStr]) groupMap[dayStr] = [];
          groupMap[dayStr].push(ls);
        }
        const days = Object.keys(groupMap).sort((a,b) => new Date(b) - new Date(a));
        for (const day of days) {
          html += `<h4>${day}</h4>`;
          const items = groupMap[day];
          for (const it of items) {
            const timeS = new Date(it.time * 1000).toLocaleTimeString();
            html += `<div>Time: ${timeS}, XP: ${it.xp}</div>`;
          }
        }
      }
      statusArea.innerHTML = html;
    }

    function setStatus(text) {
      document.getElementById("statusArea").innerText = text;
    }

    function showError(err) {
      document.getElementById("errorArea").innerHTML = `<div class="error">${err}</div>`;
    }

    function clearError() {
      document.getElementById("errorArea").innerHTML = "";
    }

    window.onload = () => {
      // initial fetch
      fetchStatus();

      // refresh button
      document.getElementById("refreshBtn").onclick = fetchStatus;

      // JWT box
      const jwtBox = document.getElementById("jwtBox");
      jwtBox.oninput = (ev) => {
        jwtInput = ev.target.value;
      };
      // JWT button
      document.getElementById("jwtBtn").onclick = updateJwt;
    };
  </script>
</body>
</html>
"#,
    )
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Status {
    xp_goal: u32,
    xp_today: u32,
    lessons_today: Vec<Lesson>,
    blocked: bool,
}

async fn get_status_inner(duo: &DuolingoApi) -> Result<Status> {
    let state = duo
        .get_daily_progress()
        .await
        .context("Error fetching duolingo status")?;

    let blocked = state.xp_today < DAILY_XP_GOAL;
    match blocked {
        true => enforcer::block_all(),
        false => enforcer::disarm(),
    }

    Ok(Status {
        xp_goal: DAILY_XP_GOAL,
        xp_today: state.xp_today,
        lessons_today: state.lessons_today,
        blocked,
    })
}

/// Duolingos and returns updated lesson / xp state
async fn get_status(State(app): State<AppState>) -> Result<Json<Status>, (StatusCode, String)> {
    let duo = app.duo.lock().await;
    let status = get_status_inner(&duo).await.map_err(internal_error)?;

    Ok(Json(status))
}

/// POST /api/update_jwt => { "new_jwt": "..." }
async fn update_jwt(
    State(app): State<AppState>,
    Json(payload): Json<UpdateJwtBody>,
) -> Result<(), (StatusCode, String)> {
    let mut duo = app.duo.lock().await;
    duo.update_jwt(&payload.new_jwt)
        .await
        .context("Failed to update JWT")
        .map_err(internal_error)?;

    if let Err(e) = tokio::fs::write(&*PERSISTENT_JWT_STORAGE_PATH, &payload.new_jwt).await {
        warn!("Failed to save new JWT token: {e:?}");
    }

    Ok(())
}

/// Maps anyhow errors to `500 Internal Server Error` + message
fn internal_error(err: anyhow::Error) -> (StatusCode, String) {
    warn!("Returning error in response: {err:?}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{err:?}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let level = env_logger::Env::default().default_filter_or("debug");
    env_logger::Builder::from_env(level).init();

    let mut jwt_token = None;
    let token_path = &*PERSISTENT_JWT_STORAGE_PATH;
    if std::fs::exists(token_path).unwrap_or(false) {
        match std::fs::read_to_string(token_path) {
            Ok(token) => {
                info!("Read {} token bytes from {token_path:?}", token.len());
                jwt_token = Some(token);
            }
            Err(e) => {
                warn!("Failed to read jwt token at {token_path:?}: {e:?}");
            }
        }
    }

    if jwt_token.is_none() {
        jwt_token = std::env::var("JWT_TOKEN").ok().and_then(|token| {
            if token.is_empty() {
                None
            } else {
                info!("Read {} token bytes from JWT_TOKEN var", token.len());
                Some(token)
            }
        });
    }

    let mut duo = DuolingoApi::new()?;

    if let Some(token) = jwt_token {
        duo.update_jwt(&token)
            .await
            .context("Failed to set inital jwt")?;

        if let Err(e) = get_status_inner(&duo).await {
            warn!("Failed to get inital xp status: {e:?}");
        }
    };

    let state = AppState {
        duo: Arc::new(Mutex::new(duo)),
    };

    let app = Router::new()
        .route("/", get(ui_handler))
        .route("/api/status", get(get_status))
        .route("/api/update_jwt", post(update_jwt))
        .with_state(state)
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::new(Duration::from_secs(2)),
        ));

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 4550));
    info!("Listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to server socket {addr}",))?;

    let r = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Failed to serve app");

    enforcer::block_all();

    r
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
