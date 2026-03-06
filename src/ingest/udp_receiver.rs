use crate::error::{Result, StreamError};
use crate::monitoring::Metrics;
use bytes::Bytes;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const MAX_UDP_PACKET_SIZE: usize = 1500;

pub struct UdpReceiver {
    bind_addr: String,
    metrics: Arc<Metrics>,
}

impl UdpReceiver {
    pub fn new(bind_addr: String, metrics: Arc<Metrics>) -> Self {
        Self { bind_addr, metrics }
    }

    pub async fn start(self, tx: mpsc::Sender<Bytes>) -> Result<()> {
        info!("Binding UDP socket to {}", self.bind_addr);
        let socket = UdpSocket::bind(&self.bind_addr).await?;
        info!("UDP socket bound successfully");

        let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
        let mut packet_count = 0u64;

        loop {
            match socket.recv(&mut buf).await {
                Ok(len) => {
                    packet_count += 1;
                    self.metrics.record_bytes(len as u64);

                    if packet_count % 1000 == 0 {
                        debug!("Received {} UDP packets", packet_count);
                    }

                    let packet = Bytes::copy_from_slice(&buf[..len]);
                    if let Err(e) = tx.send(packet).await {
                        error!("Failed to send packet to demuxer: {}", e);
                        return Err(StreamError::Network(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "Channel closed",
                        )));
                    }
                }
                Err(e) => {
                    error!("UDP receive error: {}", e);
                    return Err(StreamError::Network(e));
                }
            }
        }
    }
}
