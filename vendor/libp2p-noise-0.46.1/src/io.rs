// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Noise protocol I/O.

mod framed;
pub(crate) mod handshake;
use std::{
    cmp::min,
    fmt, io,
    pin::Pin,
    task::{Context, Poll},
};

use asynchronous_codec::Framed;
use bytes::Bytes;
use framed::{Codec, MAX_FRAME_LEN};
use futures::{prelude::*, ready};
use std::time::Instant;

/// A noise session to a remote.
///
/// `T` is the type of the underlying I/O resource.
pub struct Output<T> {
    io: Framed<T, Codec<snow::TransportState>>,
    recv_buffer: Bytes,
    recv_offset: usize,
    send_buffer: Vec<u8>,
    send_offset: usize,
    diag_pending_ready_since: Option<Instant>,
    diag_pending_innerflush_since: Option<Instant>,
    diag_last_stuck_log: Option<Instant>,
}

impl<T> fmt::Debug for Output<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoiseOutput").finish()
    }
}

impl<T> Output<T> {
    fn new(io: Framed<T, Codec<snow::TransportState>>) -> Self {
        Output {
            io,
            recv_buffer: Bytes::new(),
            recv_offset: 0,
            send_buffer: Vec::new(),
            send_offset: 0,
            diag_pending_ready_since: None,
            diag_pending_innerflush_since: None,
            diag_last_stuck_log: None,
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for Output<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let len = self.recv_buffer.len();
            let off = self.recv_offset;
            if len > 0 {
                let n = min(len - off, buf.len());
                buf[..n].copy_from_slice(&self.recv_buffer[off..off + n]);
                tracing::trace!(copied_bytes=%(off + n), total_bytes=%len, "read: copied");
                self.recv_offset += n;
                if len == self.recv_offset {
                    tracing::trace!("read: frame consumed");
                    // Drop the existing view so `NoiseFramed` can reuse
                    // the buffer when polling for the next frame below.
                    self.recv_buffer = Bytes::new();
                }
                return Poll::Ready(Ok(n));
            }

            match Pin::new(&mut self.io).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(0)),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(Some(Ok(frame))) => {
                    self.recv_buffer = frame;
                    self.recv_offset = 0;
                }
            }
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Output<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = Pin::into_inner(self);
        let mut io = Pin::new(&mut this.io);
        let frame_buf = &mut this.send_buffer;

        // The MAX_FRAME_LEN is the maximum buffer size before a frame must be sent.
        if this.send_offset == MAX_FRAME_LEN {
            tracing::trace!(bytes=%MAX_FRAME_LEN, "write: sending");
            ready!(io.as_mut().poll_ready(cx))?;
            io.as_mut().start_send(frame_buf)?;
            this.send_offset = 0;
        }

        let off = this.send_offset;
        let n = min(MAX_FRAME_LEN, off.saturating_add(buf.len()));
        this.send_buffer.resize(n, 0u8);
        let n = min(MAX_FRAME_LEN - off, buf.len());
        this.send_buffer[off..off + n].copy_from_slice(&buf[..n]);
        this.send_offset += n;
        tracing::trace!(bytes=%this.send_offset, "write: buffered");

        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = Pin::into_inner(self);
        let mut io = Pin::new(&mut this.io);
        let frame_buf = &mut this.send_buffer;

        // Check if there is still one more frame to send.
        if this.send_offset > 0 {
            match io.as_mut().poll_ready(cx) {
                Poll::Pending => {
                    let since = *this.diag_pending_ready_since.get_or_insert_with(Instant::now);
                    let elapsed = since.elapsed();
                    if elapsed.as_secs_f64() >= 1.0 {
                        let should_log = this
                            .diag_last_stuck_log
                            .map(|t| t.elapsed().as_secs_f64() >= 2.0)
                            .unwrap_or(true);
                        if should_log {
                            tracing::info!("[gossip-mesh-diag/vendor-noise] inner poll_ready (buffer-drain) STUCK for {:?} (send_offset={})", elapsed, this.send_offset);
                            this.diag_last_stuck_log = Some(Instant::now());
                        }
                    }
                    return Poll::Pending;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                Poll::Ready(Ok(())) => {
                    if let Some(since) = this.diag_pending_ready_since.take() {
                        tracing::info!("[gossip-mesh-diag/vendor-noise] inner poll_ready UNSTUCK after {:?}", since.elapsed());
                    }
                }
            }
            tracing::trace!(bytes= %this.send_offset, "flush: sending");
            io.as_mut().start_send(frame_buf)?;
            this.send_offset = 0;
        }

        match io.as_mut().poll_flush(cx) {
            Poll::Pending => {
                let since = *this.diag_pending_innerflush_since.get_or_insert_with(Instant::now);
                let elapsed = since.elapsed();
                if elapsed.as_secs_f64() >= 1.0 {
                    let should_log = this
                        .diag_last_stuck_log
                        .map(|t| t.elapsed().as_secs_f64() >= 2.0)
                        .unwrap_or(true);
                    if should_log {
                        tracing::info!("[gossip-mesh-diag/vendor-noise] inner (raw io) poll_flush STUCK for {:?}", elapsed);
                        this.diag_last_stuck_log = Some(Instant::now());
                    }
                }
                Poll::Pending
            }
            Poll::Ready(res) => {
                if let Some(since) = this.diag_pending_innerflush_since.take() {
                    tracing::info!("[gossip-mesh-diag/vendor-noise] inner (raw io) poll_flush UNSTUCK after {:?}", since.elapsed());
                }
                Poll::Ready(res)
            }
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        Pin::new(&mut self.io).poll_close(cx)
    }
}
