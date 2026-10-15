// SPDX-License-Identifier: MPL-2.0
//! TCP adapters.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use dropumi_application::error::TransportError;
use dropumi_application::ports::{Connector, Listener};
use tokio::net::{TcpListener, TcpStream};

/// Default receive port.
pub const DEFAULT_PORT: u16 = 47_321;

fn classify(e: io::Error) -> TransportError {
    match e.kind() {
        io::ErrorKind::ConnectionRefused => TransportError::Refused,
        io::ErrorKind::HostUnreachable | io::ErrorKind::NetworkUnreachable => {
            TransportError::Unreachable
        }
        io::ErrorKind::TimedOut => TransportError::Timeout,
        _ => TransportError::Io(e),
    }
}

/// Outgoing TCP connections.
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpConnector;

impl Connector for TcpConnector {
    type Stream = TcpStream;

    async fn connect(
        &self,
        addr: SocketAddr,
        timeout: Duration,
    ) -> Result<TcpStream, TransportError> {
        let stream = tokio::time::timeout(timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(classify)?;
        let _ = stream.set_nodelay(true);
        Ok(stream)
    }

    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, TransportError> {
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| TransportError::Resolve)?
            .collect();
        if addrs.is_empty() {
            return Err(TransportError::Resolve);
        }
        Ok(addrs)
    }
}

/// An incoming TCP listener.
#[derive(Debug)]
pub struct TcpListenerAdapter {
    inner: TcpListener,
}

impl TcpListenerAdapter {
    /// Binds `ip:port`. When `port` is the default and busy (and was not
    /// chosen explicitly), falls back to an ephemeral port.
    ///
    /// # Errors
    /// [`TransportError::Bind`] if binding fails.
    pub async fn bind(ip: IpAddr, port: u16, explicit_port: bool) -> Result<Self, TransportError> {
        match TcpListener::bind((ip, port)).await {
            Ok(inner) => Ok(Self { inner }),
            Err(e) if e.kind() == io::ErrorKind::AddrInUse && !explicit_port && port != 0 => {
                tracing::info!(port, "default port busy; using an ephemeral port");
                TcpListener::bind((ip, 0))
                    .await
                    .map(|inner| Self { inner })
                    .map_err(|e| TransportError::Bind(e.to_string()))
            }
            Err(e) => Err(TransportError::Bind(e.to_string())),
        }
    }

    /// The bound port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.inner.local_addr().map_or(0, |a| a.port())
    }
}

impl Listener for TcpListenerAdapter {
    type Stream = TcpStream;

    async fn accept(&mut self) -> Result<(TcpStream, SocketAddr), TransportError> {
        let (stream, addr) = self.inner.accept().await.map_err(TransportError::Io)?;
        let _ = stream.set_nodelay(true);
        Ok((stream, addr))
    }

    fn local_addrs(&self) -> Vec<SocketAddr> {
        self.inner.local_addr().into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[tokio::test]
    async fn listener_falls_back_to_ephemeral_when_port_busy() {
        let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let first = TcpListenerAdapter::bind(lo, 0, true).await.unwrap();
        let busy = first.port();
        // Default (not explicit) port busy: fall back.
        let second = TcpListenerAdapter::bind(lo, busy, false).await.unwrap();
        assert_ne!(second.port(), busy);
        // Explicit port busy: error.
        assert!(TcpListenerAdapter::bind(lo, busy, true).await.is_err());
    }

    #[tokio::test]
    async fn connect_first_skips_dead_candidate() {
        let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
        // Port 1 is privileged, so nothing a test starts can listen there. (A
        // released ephemeral port is not safe: the next bind may reuse it.)
        let dead = SocketAddr::new(lo, 1);
        let live = TcpListenerAdapter::bind(lo, 0, true).await.unwrap();
        let live_addr = SocketAddr::new(lo, live.port());
        let c = TcpConnector;
        assert!(c.connect(dead, Duration::from_secs(3)).await.is_err());
        assert!(c.connect(live_addr, Duration::from_secs(3)).await.is_ok());
    }
}
