//! `GET /api/events` — Server-Sent Events stream of live-update pings.
//!
//! The browser opens one of these per tab (see the root layout). Every
//! debounced file change posts a unit value to the broadcast channel on
//! [`AppState`]; this handler subscribes a fresh
//! receiver per connection and forwards each as a contentless
//! `data: changed` SSE event. The payload carries no detail on purpose:
//! per the live-updates decisions the client simply re-fetches the
//! current page's data on any ping rather than acting on event specifics
//! (a single item edit can ripple into other items' computed values, so
//! whole-view refetch is both simpler and always correct).
//!
//! Cleanup is automatic: closing the tab drops the response stream, which
//! drops the broadcast receiver and unsubscribes it — no connection
//! bookkeeping, no leaks. A keep-alive comment holds an otherwise-idle
//! connection open.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::state::AppState;

/// Router for the `/events` endpoint under `/api`.
pub fn router() -> Router<AppState> {
    Router::new().route("/events", get(events))
}

async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.events.subscribe();

    // Each received ping maps to one contentless "changed" event. A
    // `Lagged` error (the browser fell behind and overflowed the buffer)
    // equally means "something changed, refetch", so it maps the same way.
    let stream = BroadcastStream::new(receiver)
        .map(|_result| Ok::<_, Infallible>(Event::default().data("changed")));

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    use crate::state::AppState;

    /// The endpoint is wired and returns an SSE stream. We assert on the
    /// response head only — consuming the streaming body would block, and
    /// the ping-delivery path is covered by the broadcast channel's own
    /// guarantees plus the watcher's pure unit tests (kept off the
    /// filesystem to stay non-flaky).
    #[tokio::test]
    async fn events_endpoint_returns_event_stream() {
        let app = crate::api::router().with_state(AppState::test_stub());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
    }
}
