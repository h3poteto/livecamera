use std::collections::HashMap;
use std::sync::Mutex;

use actix_web::web::{Data, Query};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod room;
mod websocket;
mod worker;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let worker_owner = worker::WorkerOwner::new().await;
    let worker_data = Data::new(worker_owner);

    let room_owner = room::RoomOwner::new();
    let room_data = Data::new(Mutex::new(room_owner));

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .service(index)
            .app_data(worker_data.clone())
            .app_data(room_data.clone())
            .route("/socket", web::get().to(socket))
    })
    .bind("0.0.0.0:4000")?
    .run()
    .await
}

#[actix_web::get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("healthy")
}

async fn socket(
    req: HttpRequest,
    worker_owner: Data<worker::WorkerOwner>,
    room_owner: Data<Mutex<room::RoomOwner>>,
    stream: web::Payload,
) -> impl Responder {
    let query = req.query_string();

    let parameters =
        Query::<HashMap<String, String>>::from_query(query).expect("Failed to parse query");
    let room_id = parameters.get("room").expect("room is required");

    let find = room_owner
        .as_ref()
        .lock()
        .unwrap()
        .find_by_id(room_id.to_string());
    let worker = worker_owner.choose_worker().expect("No worker available");

    match find {
        Some(room) => {
            tracing::info!("Room found, so joining it: {}", room_id);
            let server = websocket::WebSocket::new(room).await;
            ws::start(server, &req, stream)
        }
        None => {
            let owner = room_owner.clone();
            let mut owner = owner.lock().unwrap();
            let room = owner.create_new_room(room_id.to_string(), worker).await;
            let server = websocket::WebSocket::new(room).await;
            ws::start(server, &req, stream)
        }
    }
}
