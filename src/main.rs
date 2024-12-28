use axum::Router;
use leptos::{config::get_configuration, logging};
use leptos_axum::{generate_route_list, LeptosRoutes};
// use server_fns_axum::*;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setting this to None means we'll be using cargo-leptos and its env vars
    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(duo_enforcer::ui::App);

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || duo_enforcer::ui::shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(duo_enforcer::ui::shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    logging::log!("listening on http://{}", &addr);
    Ok(axum::serve(listener, app.into_make_service()).await?)
}
