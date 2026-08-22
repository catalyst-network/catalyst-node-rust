// Copyright 2021 Protocol Labs.
// Copyright 2018 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Async functions driving pending and established connections in the form of a task.

use std::{convert::Infallible, pin::Pin, task::Poll};

use futures::{
    channel::{mpsc, oneshot},
    future::{poll_fn, Either, Future},
    SinkExt, StreamExt,
};
use libp2p_core::muxing::StreamMuxerBox;

use super::concurrent_dial::ConcurrentDial;
use crate::{
    connection::{
        self, ConnectionError, ConnectionId, PendingInboundConnectionError,
        PendingOutboundConnectionError,
    },
    transport::TransportError,
    ConnectionHandler, Multiaddr, PeerId,
};

/// Commands that can be sent to a task driving an established connection.
#[derive(Debug)]
pub(crate) enum Command<T> {
    /// Notify the connection handler of an event.
    NotifyHandler(T),
    /// Gracefully close the connection (active close) before
    /// terminating the task.
    Close,
}

pub(crate) enum PendingConnectionEvent {
    ConnectionEstablished {
        id: ConnectionId,
        output: (PeerId, StreamMuxerBox),
        /// [`Some`] when the new connection is an outgoing connection.
        /// Addresses are dialed in parallel. Contains the addresses and errors
        /// of dial attempts that failed before the one successful dial.
        outgoing: Option<(Multiaddr, Vec<(Multiaddr, TransportError<std::io::Error>)>)>,
    },
    /// A pending connection failed.
    PendingFailed {
        id: ConnectionId,
        error: Either<PendingOutboundConnectionError, PendingInboundConnectionError>,
    },
}

#[derive(Debug)]
pub(crate) enum EstablishedConnectionEvent<ToBehaviour> {
    /// A node we are connected to has changed its address.
    AddressChange {
        id: ConnectionId,
        peer_id: PeerId,
        new_address: Multiaddr,
    },
    /// Notify the manager of an event from the connection.
    Notify {
        id: ConnectionId,
        peer_id: PeerId,
        event: ToBehaviour,
    },
    /// A connection closed, possibly due to an error.
    ///
    /// If `error` is `None`, the connection has completed
    /// an active orderly close.
    Closed {
        id: ConnectionId,
        peer_id: PeerId,
        error: Option<ConnectionError>,
    },
}

pub(crate) async fn new_for_pending_outgoing_connection(
    connection_id: ConnectionId,
    dial: ConcurrentDial,
    abort_receiver: oneshot::Receiver<Infallible>,
    mut events: mpsc::Sender<PendingConnectionEvent>,
) {
    match futures::future::select(abort_receiver, Box::pin(dial)).await {
        Either::Left((Err(oneshot::Canceled), _)) => {
            let _ = events
                .send(PendingConnectionEvent::PendingFailed {
                    id: connection_id,
                    error: Either::Left(PendingOutboundConnectionError::Aborted),
                })
                .await;
        }
        Either::Left((Ok(v), _)) => libp2p_core::util::unreachable(v),
        Either::Right((Ok((address, output, errors)), _)) => {
            let _ = events
                .send(PendingConnectionEvent::ConnectionEstablished {
                    id: connection_id,
                    output,
                    outgoing: Some((address, errors)),
                })
                .await;
        }
        Either::Right((Err(e), _)) => {
            let _ = events
                .send(PendingConnectionEvent::PendingFailed {
                    id: connection_id,
                    error: Either::Left(PendingOutboundConnectionError::Transport(e)),
                })
                .await;
        }
    }
}

pub(crate) async fn new_for_pending_incoming_connection<TFut>(
    connection_id: ConnectionId,
    future: TFut,
    abort_receiver: oneshot::Receiver<Infallible>,
    mut events: mpsc::Sender<PendingConnectionEvent>,
) where
    TFut: Future<Output = Result<(PeerId, StreamMuxerBox), std::io::Error>> + Send + 'static,
{
    match futures::future::select(abort_receiver, Box::pin(future)).await {
        Either::Left((Err(oneshot::Canceled), _)) => {
            let _ = events
                .send(PendingConnectionEvent::PendingFailed {
                    id: connection_id,
                    error: Either::Right(PendingInboundConnectionError::Aborted),
                })
                .await;
        }
        Either::Left((Ok(v), _)) => libp2p_core::util::unreachable(v),
        Either::Right((Ok(output), _)) => {
            let _ = events
                .send(PendingConnectionEvent::ConnectionEstablished {
                    id: connection_id,
                    output,
                    outgoing: None,
                })
                .await;
        }
        Either::Right((Err(e), _)) => {
            let _ = events
                .send(PendingConnectionEvent::PendingFailed {
                    id: connection_id,
                    error: Either::Right(PendingInboundConnectionError::Transport(
                        TransportError::Other(e),
                    )),
                })
                .await;
        }
    }
}

pub(crate) async fn new_for_established_connection<THandler>(
    connection_id: ConnectionId,
    peer_id: PeerId,
    mut connection: crate::connection::Connection<THandler>,
    mut command_receiver: mpsc::Receiver<Command<THandler::FromBehaviour>>,
    mut events: mpsc::Sender<EstablishedConnectionEvent<THandler::ToBehaviour>>,
) where
    THandler: ConnectionHandler,
{
    loop {
        // FLUSH-STALL FIX (2026-08-22, see project_flush_stall_fix_2026-08-22.md): the previous
        // `futures::future::select(command_receiver.next(), poll_fn(connection.poll))` let a
        // continuously-ready `command_receiver` (e.g. a burst of outbound NotifyHandler commands,
        // which is exactly what heavy publish/relay traffic produces) win this race on *every*
        // poll, forever -- `future::select` returns as soon as either side is Ready, so
        // `connection.poll(cx)` was never even called during such a burst, not just starved
        // internally. Since `connection.poll()`'s side effect (via `muxing.poll_unpin`, see the
        // separate fix in `connection.rs`) is the only thing that flushes queued yamux writes,
        // this fully explains the observed `PendingFlush STUCK` symptom persisting even after that
        // first fix landed (confirmed live: 765s+ stuck, same build). Poll both unconditionally on
        // every call instead, so the connection is always driven forward regardless of how much
        // command traffic is arriving; command handling still gets priority when both are ready
        // (this loop's the same as before otherwise), but it can no longer starve the connection
        // poll's call itself.
        match poll_fn(|cx| {
            let conn_poll = Pin::new(&mut connection).poll(cx);
            let cmd_poll = command_receiver.poll_next_unpin(cx);

            if let Poll::Ready(command) = cmd_poll {
                return Poll::Ready(Either::Left(command));
            }
            if let Poll::Ready(event) = conn_poll {
                return Poll::Ready(Either::Right(event));
            }
            Poll::Pending
        })
        .await
        {
            Either::Left(Some(command)) => match command {
                Command::NotifyHandler(event) => connection.on_behaviour_event(event),
                Command::Close => {
                    command_receiver.close();
                    let (remaining_events, closing_muxer) = connection.close();

                    let _ = events
                        .send_all(&mut remaining_events.map(|event| {
                            Ok(EstablishedConnectionEvent::Notify {
                                id: connection_id,
                                event,
                                peer_id,
                            })
                        }))
                        .await;

                    let error = closing_muxer.await.err().map(ConnectionError::IO);

                    let _ = events
                        .send(EstablishedConnectionEvent::Closed {
                            id: connection_id,
                            peer_id,
                            error,
                        })
                        .await;
                    return;
                }
            },

            // The manager has disappeared; abort.
            Either::Left(None) => return,

            Either::Right(event) => {
                match event {
                    Ok(connection::Event::Handler(event)) => {
                        // TEMPORARY DIAGNOSTIC (2026-08-20, flush-stall investigation): confirm
                        // or refute the theory that this per-connection task blocks here on
                        // event delivery backpressure, which would prevent it from ever looping
                        // back to poll the muxer (and thus flush outbound data) -- see
                        // connection.rs:404's muxing.poll_unpin, only reached after this send
                        // resolves.
                        let diag_send_start = std::time::Instant::now();
                        let _ = events
                            .send(EstablishedConnectionEvent::Notify {
                                id: connection_id,
                                peer_id,
                                event,
                            })
                            .await;
                        let diag_elapsed = diag_send_start.elapsed();
                        if diag_elapsed.as_secs_f64() >= 1.0 {
                            tracing::info!(
                                "[gossip-mesh-diag/vendor-swarm] events.send(Notify) for peer={:?} conn={:?} took {:?} (blocked the connection task from reaching muxer poll)",
                                peer_id, connection_id, diag_elapsed
                            );
                        }
                    }
                    Ok(connection::Event::AddressChange(new_address)) => {
                        let _ = events
                            .send(EstablishedConnectionEvent::AddressChange {
                                id: connection_id,
                                peer_id,
                                new_address,
                            })
                            .await;
                    }
                    Err(error) => {
                        command_receiver.close();
                        let (remaining_events, _closing_muxer) = connection.close();

                        let _ = events
                            .send_all(&mut remaining_events.map(|event| {
                                Ok(EstablishedConnectionEvent::Notify {
                                    id: connection_id,
                                    event,
                                    peer_id,
                                })
                            }))
                            .await;

                        // Terminate the task with the error, dropping the connection.
                        let _ = events
                            .send(EstablishedConnectionEvent::Closed {
                                id: connection_id,
                                peer_id,
                                error: Some(error),
                            })
                            .await;
                        return;
                    }
                }
            }
        }
    }
}
