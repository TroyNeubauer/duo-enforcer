use crate::*;
use chrono::{Local, TimeZone};
use leptos::prelude::*;
use leptos::task::spawn_local;
use server_fn::error::NoCustomError;
use std::time::Duration;

#[cfg(feature = "ssr")]
fn try_send_command(cmd: ActorCommand) -> anyhow::Result<()> {
    match crate::duo_driver::CMD_TX.get().map(|tx| tx.try_send(cmd)) {
        Some(Ok(())) => Ok(()),
        Some(Err(e)) => Err(e.into()),
        None => anyhow::bail!("CMD_TX not set"),
    }
}

#[server(GetStatus, "/api")]
pub async fn get_status() -> Result<SharedState, ServerFnError> {
    let st = crate::duo_driver::SHARED_STATE.lock().unwrap().clone();
    Ok(st)
}

#[server(ForcePollNow, "/api")]
pub async fn force_poll_now() -> Result<(), ServerFnError> {
    try_send_command(ActorCommand::ForcePoll).map_err(|e| {
        ServerFnError::<NoCustomError>::ServerError(format!("Failed to send command: {e}"))
    })
}

#[server(UpdateJwt, "/api")]
pub async fn update_jwt_server(new_jwt: String) -> Result<(), ServerFnError> {
    try_send_command(ActorCommand::UpdateJWT(new_jwt)).map_err(|e| {
        ServerFnError::<NoCustomError>::ServerError(format!("Failed to send command: {e}"))
    })
}

/// The main Leptos app component: displays current status, JWT input, etc.
#[component]
pub fn App() -> impl IntoView {
    // Create a resource that fetches the current status from `get_status` server fn.
    let status_res = Resource::new(|| (), |_| get_status());

    // We'll poll it every 5s automatically
    set_interval(
        move || {
            status_res.refetch();
        },
        Duration::from_secs(5),
    );

    // For typed JWT input
    let (jwt_input, set_jwt_input) = signal(String::new());

    // A helper to display time to midnight
    fn time_until_midnight() -> String {
        let now = Local::now();
        let midnight = now
            .date_naive()
            .succ_opt() // next day
            .unwrap_or(now.date_naive())
            .and_hms_opt(0, 0, 0)
            .unwrap_or(now.naive_local());
        let diff = midnight - now.naive_local();
        let hours = diff.num_hours();
        let minutes = (diff.num_minutes() % 60).abs();
        format!("{hours}h {minutes}m")
    }

    view! {
        <h1>"Duolingo Enforcer"</h1>
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            {move || {
                status_res.read().map(|maybe_status| {
                    match maybe_status {
                        Ok(status) => {
                            let color = if status.blocked { "red" } else { "green" };
                            let main_text = if status.blocked {
                                format!("BLOCKED! XP: {}/{}", status.xp_today, status.xp_goal)
                            } else {
                                format!("UNBLOCKED! XP: {}/{}", status.xp_today, status.xp_goal)
                            };
                            let parted = if !status.blocked {
                                format!("(Time til midnight: {})", time_until_midnight())
                            } else {
                                "".to_string()
                            };
                            let err_html = status.last_error.clone().unwrap_or_default();

                            // Group lessons by day
                            let mut day_map = std::collections::BTreeMap::new();
                            for les in &status.lessons {
                                let day_str = Local
                                    .timestamp_opt(les.time, 0)
                                    .single()
                                    .unwrap_or(Local::now())
                                    .format("%Y-%m-%d")
                                    .to_string();
                                day_map.entry(day_str).or_insert(vec![]).push(les);
                            }

                            view! {
                                <div>
                                    <p style=format!("color: {}", color) >
                                        {main_text} " " {parted}
                                    </p>
                                    <p style="color:red;">{err_html}</p>
                                    <button on:click=move |_| {
                                        spawn_local(async move {
                                            let _ = force_poll_now().await;
                                            status_res.refetch();
                                        });
                                    }>"Scan Now"</button>

                                    <div style="margin-top: 1em;">
                                        <input
                                            placeholder="New JWT..."
                                            prop:value=jwt_input
                                            on:input=move |ev| set_jwt_input.set(event_target_value(&ev))
                                            style="width: 300px;"
                                        />
                                        <button on:click=move |_| {
                                            let new_jwt = jwt_input.get_untracked();
                                            spawn_local(async move {
                                                let _ = update_jwt_server(new_jwt.clone()).await;
                                                status_res.refetch();
                                            });
                                        }>"Update JWT"</button>
                                    </div>

                                    <hr/>
                                    <h2>"Recent Lessons"</h2>
                                    {
                                        day_map.into_iter().rev().map(move |(day, lessons)| {
                                            // For each day, we display day heading and each lesson
                                            view! {
                                                <div style="margin-top:1em;">
                                                    <h3>{day.clone()}</h3>
                                                    { lessons.iter().map(move |lesson| {
                                                        let local_time = Local
                                                            .timestamp_opt(lesson.time, 0)
                                                            .single()
                                                            .unwrap_or(Local::now())
                                                            .format("%H:%M:%S")
                                                            .to_string();
                                                        view! {
                                                            <p>{local_time}": XP=" {lesson.xp}</p>
                                                        }
                                                    }).collect_view() }
                                                    <hr/>
                                                </div>
                                            }
                                        }).collect_view()
                                    }
                                </div>
                            }
                            .into_view()
                        }
                        Err(e) => view! {
                            <p style="color:red;">{"Error getting status: "} {e.to_string()}</p>
                        }.into_view(),
                    }
                })
            }}
        </Suspense>
    }
}
