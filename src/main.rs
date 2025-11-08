use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rand::{Rng, rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, VecDeque}, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Profile {
    id: Uuid,
    name: String,
    // additional fields can be added: mmr, avatar, etc.
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MatchInfo {
    id: Uuid,
    player1: Uuid,
    player2: Uuid,
}

struct AppState {
    profiles: Mutex<HashMap<Uuid, Profile>>,
    queue: Mutex<VecDeque<Uuid>>,
    matches: Mutex<HashMap<Uuid, MatchInfo>>,
}

#[derive(Debug, Deserialize)]
struct CreateProfile {
    name: String,
}

#[derive(Debug, Deserialize)]
struct QueueRequest {
    profile_id: Uuid,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState {
        profiles: Mutex::new(HashMap::new()),
        queue: Mutex::new(VecDeque::new()),
        matches: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route(
            "/profiles",
            post(|State(state): State<Arc<AppState>>, Json(payload): Json<CreateProfile>| async move {
                create_profile(State(state), Json(payload)).await
            }),
        )
        .route(
            "/profiles/:id",
            get(|State(state): State<Arc<AppState>>, Path(id): Path<Uuid>| async move {
                get_profile(State(state), Path(id)).await
            }),
        )
        .route(
            "/queue/enqueue",
            post(|State(state): State<Arc<AppState>>, Json(payload): Json<QueueRequest>| async move {
                enqueue(State(state), Json(payload)).await
            }),
        )
        .route(
            "/queue/leave",
            post(|State(state): State<Arc<AppState>>, Json(payload): Json<QueueRequest>| async move {
                leave_queue(State(state), Json(payload)).await
            }),
        )
        .route(
            "/queue",
            get(|State(state): State<Arc<AppState>>| async move { get_queue(State(state)).await }),
        )
        .route(
            "/matches",
            get(|State(state): State<Arc<AppState>>| async move { list_matches(State(state)).await }),
        )
        .route(
            "/matches/:id",
            get(|State(state): State<Arc<AppState>>, Path(id): Path<Uuid>| async move {
                get_match(State(state), Path(id)).await
            }),
        )
        .with_state(state.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateProfile>,
) -> Response {
    let id = Uuid::new_v4();
    let profile = Profile {
        id,
        name: payload.name,
    };
    let mut map = state.profiles.lock().await;
    map.insert(id, profile.clone());
    (StatusCode::CREATED, Json(profile)).into_response()
}

async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Response {
    let map = state.profiles.lock().await;
    if let Some(p) = map.get(&id) {
        (StatusCode::OK, Json(p.clone())).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Profile not found").into_response()
    }
}

async fn enqueue(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueueRequest>,
) -> Response {
    // Ensure profile exists
    let profiles = state.profiles.lock().await;
    if !profiles.contains_key(&payload.profile_id) {
        return (StatusCode::BAD_REQUEST, "Profile does not exist").into_response();
    }
    drop(profiles);

    // Add to queue if not already present
    let mut queue = state.queue.lock().await;
    if queue.contains(&payload.profile_id) {
        return (StatusCode::OK, "Already in queue").into_response();
    }

    // If queue has someone waiting, pick a random opponent from queue
    if !queue.is_empty() {
        // choose random opponent from existing queue
        // use StdRng (Send) to avoid holding a non-Send ThreadRng across awaits
        let mut rng = StdRng::from_entropy();
        if queue.len() > 0 {
            let idx = rng.gen_range(0..queue.len());
            let opponent_id = queue.remove(idx).unwrap();
            // create match
            let m = MatchInfo {
                id: Uuid::new_v4(),
                player1: opponent_id,
                player2: payload.profile_id,
            };
            let mut matches = state.matches.lock().await;
            matches.insert(m.id, m.clone());
            return (StatusCode::CREATED, Json(m)).into_response();
        }
    }

    // otherwise push to queue
    queue.push_back(payload.profile_id);
    (StatusCode::ACCEPTED, "Enqueued").into_response()
}

async fn leave_queue(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueueRequest>,
) -> Response {
    let mut queue = state.queue.lock().await;
    if let Some(pos) = queue.iter().position(|id| *id == payload.profile_id) {
        queue.remove(pos);
        (StatusCode::OK, "Removed from queue").into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Not in queue").into_response()
    }
}

async fn get_queue(State(state): State<Arc<AppState>>) -> Response {
    let queue = state.queue.lock().await;
    let list: Vec<Uuid> = queue.iter().cloned().collect();
    (StatusCode::OK, Json(list)).into_response()
}

async fn list_matches(State(state): State<Arc<AppState>>) -> Response {
    let matches = state.matches.lock().await;
    let list: Vec<MatchInfo> = matches.values().cloned().collect();
    (StatusCode::OK, Json(list)).into_response()
}

async fn get_match(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Response {
    let matches = state.matches.lock().await;
    if let Some(m) = matches.get(&id) {
        (StatusCode::OK, Json(m.clone())).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Match not found").into_response()
    }
}
