use std::time::Instant;

use actix::*;
use actix_web::{middleware::Logger, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

mod server;
mod session;

/// Entry point for our websocket route
async fn route(
    req: HttpRequest,
    stream: web::Payload,
    srv: web::Data<Addr<server::WsServer>>,
) -> Result<HttpResponse, Error> {
    log::debug!("web route request: {:?}", req);

    ws::start(
        session::WsSession {
            id: 0,
            heartbeat: Instant::now(),
            room: "main".to_owned(),
            name: None,
            addr: srv.get_ref().clone(),
        },
        &req,
        stream,
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    std::env::set_var("RUST_BACKTRACE", "1");
    let server_port = 3000;
    // start chat server actor
    let server = server::WsServer::new().start();

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(server.clone()))
            .route("/", web::get().to(route))
            .wrap(Logger::default())
    })
    .bind(("0.0.0.0", server_port))?
    .run();

    log::info!("starting HTTP server at http://localhost:{}", server_port);

    server.await
}
