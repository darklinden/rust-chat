use anyhow::{anyhow, Result};
use futures_util::{pin_mut, SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

pub struct WsClient {
    sender_tx: futures_channel::mpsc::UnboundedSender<Message>,
    recver_rx: futures_channel::mpsc::UnboundedReceiver<Message>,
    is_connected: Arc<AtomicBool>,
}

impl WsClient {
    pub fn is_connected(&self) -> bool {
        let is_connected = &*self.is_connected;
        is_connected.load(Ordering::Relaxed)
    }

    pub fn set_connected(&self, connected: bool) {
        let is_connected = &*self.is_connected;
        is_connected.swap(connected, Ordering::Relaxed);
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        if !self.is_connected() {
            return Err(anyhow!("Not connected"));
        }
        if self.sender_tx.is_closed() {
            return Err(anyhow!("Sender closed"));
        }
        self.sender_tx.unbounded_send(msg).unwrap();
        Ok(())
    }

    pub fn recv(&mut self) -> Result<Message> {
        let result = self.recver_rx.try_next();
        if result.is_err() {
            return Err(anyhow!("No message"));
        }
        let result = result.unwrap();
        if result.is_none() {
            return Err(anyhow!("No message"));
        }
        Ok(result.unwrap())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if !self.is_connected() {
            return Ok(());
        }
        let close_packet = Message::Close(None);
        self.send(close_packet).await?;

        self.set_connected(false);
        Ok(())
    }

    pub async fn new(url: &String) -> Result<WsClient> {
        let url = url::Url::parse(url).unwrap();

        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
        // println!("WebSocket handshake has been successfully completed");

        let (sender_tx, sender_rx) = futures_channel::mpsc::unbounded::<Message>();
        let (recver_tx, recver_rx) = futures_channel::mpsc::unbounded::<Message>();

        let (write, read) = ws_stream.split();

        let is_connected = Arc::new(AtomicBool::new(true));

        let is_connected_clone = is_connected.clone();
        tokio::spawn(async move {
            pin_mut!(write);
            pin_mut!(sender_rx);
            loop {
                let message = sender_rx.next().await;
                if message.is_none() {
                    continue;
                }
                let message = message.unwrap();
                write.send(message).await.unwrap();

                let is_connected = &*is_connected_clone;
                if is_connected.load(Ordering::Relaxed) == false {
                    break;
                }
            }

            // println!("Sender closed");
            write.close().await.unwrap();
        });

        let is_connected_clone = is_connected.clone();
        tokio::spawn(async move {
            pin_mut!(read);
            loop {
                let message = read.next().await;
                if message.is_none() {
                    continue;
                }
                let message = message.unwrap();
                if message.is_err() {
                    break;
                }
                let message = message.unwrap();
                if recver_tx.is_closed() {
                    break;
                }
                recver_tx.unbounded_send(message).unwrap();
            }

            // println!("Recver closed");
            let is_connected = &*is_connected_clone;
            is_connected.swap(false, Ordering::Relaxed);
            recver_tx.close_channel();
        });

        let is_connected_clone = is_connected.clone();
        let sender_tx_clone = sender_tx.clone();
        tokio::spawn(async move {
            loop {
                let is_connected = &*is_connected_clone;
                if is_connected.load(Ordering::Relaxed) == false {
                    break;
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                // println!("Send ping packet");
                let ping_packet = Message::Ping(vec![]);
                if sender_tx_clone.is_closed() {
                    break;
                }
                sender_tx_clone.unbounded_send(ping_packet).unwrap();
            }
        });

        Ok(WsClient {
            sender_tx,
            recver_rx,
            is_connected,
        })
    }
}
