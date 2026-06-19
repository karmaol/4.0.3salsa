//! Co-located pre-TPU transaction stream.
//!
//! While this validator is the leader, the scheduler streams its own incoming
//! (non-vote) transactions to a connected strategy server. This module hosts
//! the `BackrunService` gRPC server and the broadcast channel bridging the
//! synchronous scheduler/TPU thread to the async runtime.
//!
//! This is a **one-way stream**: it does not accept transactions back and does
//! not modify block production. Leader gating lives in the scheduler/TPU
//! threads via a shared `is_leader` flag; this server is a thin transport.

pub mod proto {
    tonic::include_proto!("backrun");
}

use bytes::Bytes;
use log::{info, warn};
use proto::backrun_service_server::{BackrunService, BackrunServiceServer};
use proto::subscribe_backruns_response::UpdateOneof;
use proto::{
    SubscribeBackrunsRequest, SubscribeBackrunsResponse, SubscribeUpdatePing, TransactionMessage,
};
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

/// Broadcast capacity for the outbound transaction stream.
pub const TX_OUT_CAPACITY: usize = 8192;
/// Keepalive ping interval on the subscription.
const PING_INTERVAL: Duration = Duration::from_secs(5);

/// gRPC service that streams the leader's transactions to subscribers.
#[derive(Clone)]
pub struct BackrunBridge {
    tx_out: broadcast::Sender<Bytes>,
}

impl BackrunBridge {
    pub fn new(tx_out: broadcast::Sender<Bytes>) -> Self {
        Self { tx_out }
    }
}

#[tonic::async_trait]
impl BackrunService for BackrunBridge {
    type SubscribeBackrunsStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeBackrunsResponse, Status>> + Send>>;

    async fn subscribe_backruns(
        &self,
        request: Request<Streaming<SubscribeBackrunsRequest>>,
    ) -> Result<Response<Self::SubscribeBackrunsStream>, Status> {
        info!("subscriber connected: {:?}", request.remote_addr());

        // Drain client keepalives.
        let mut inbound = request.into_inner();
        tokio::spawn(async move { while let Ok(Some(_)) = inbound.message().await {} });

        let mut rx = self.tx_out.subscribe();
        let (tx, out_rx) = mpsc::channel::<Result<SubscribeBackrunsResponse, Status>>(1024);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(PING_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    received = rx.recv() => match received {
                        Ok(data) => {
                            if tx.send(Ok(tx_update(data))).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("stream lagged, skipped {skipped} txs");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    },
                    _ = tick.tick() => {
                        if tx.send(Ok(ping_update())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(out_rx))))
    }
}

/// Serve the transaction-stream gRPC endpoint until shutdown.
pub async fn serve(addr: SocketAddr, bridge: BackrunBridge) {
    info!("transaction stream listening on {addr}");
    if let Err(e) = Server::builder()
        .add_service(BackrunServiceServer::new(bridge))
        .serve(addr)
        .await
    {
        warn!("stream server exited: {e}");
    }
}

fn now_ts() -> prost_types::Timestamp {
    prost_types::Timestamp::from(SystemTime::now())
}

fn tx_update(data: Bytes) -> SubscribeBackrunsResponse {
    SubscribeBackrunsResponse {
        ts: Some(now_ts()),
        update_oneof: Some(UpdateOneof::Transaction(TransactionMessage {
            content: data.to_vec(),
        })),
    }
}

fn ping_update() -> SubscribeBackrunsResponse {
    SubscribeBackrunsResponse {
        ts: Some(now_ts()),
        update_oneof: Some(UpdateOneof::Ping(SubscribeUpdatePing {})),
    }
}
