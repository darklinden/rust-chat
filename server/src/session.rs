use actix::prelude::*;
use actix_web_actors::ws;
use proto::ChatPacket;
use std::time::Duration;
use std::time::Instant;

use crate::server;

/// How often heartbeat pings are sent
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct WsSession {
    /// unique session id
    pub id: usize,

    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    pub heartbeat: Instant,

    /// joined room
    pub room: String,

    /// peer name
    pub name: Option<String>,

    /// Chat server
    pub addr: Addr<server::WsServer>,
}

impl WsSession {
    /// helper method that sends ping to client every 5 seconds (HEARTBEAT_INTERVAL).
    ///
    /// also this method checks heartbeats from client
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // notify chat server
                act.addr.do_send(server::Disconnect { id: act.id });

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }
        });
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start.
    /// We register ws session with ChatServer
    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.heartbeat(ctx);

        // register self in chat server. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        // HttpContext::state() is instance of WsChatSessionState, state is shared
        // across all routes within application
        let addr: Addr<WsSession> = ctx.address();
        self.addr
            .send(server::Connect {
                addr: addr.recipient(),
            })
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(res) => {
                        log::debug!("ws session started: {}", res);
                        act.id = res;
                    }
                    // something is wrong with chat server
                    _ => ctx.stop(),
                }
                fut::ready(())
            })
            .wait(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        log::debug!("ws session stopping: {}", self.id);

        // notify chat server
        self.addr.do_send(server::Disconnect { id: self.id });
        Running::Stop
    }
}

/// WebSocket message handler
/// for response client requests
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        match msg {
            ws::Message::Ping(_) => {
                // ping
                self.heartbeat = Instant::now();
                ctx.pong(&[]);
            }
            ws::Message::Pong(_) => (),
            ws::Message::Text(_) => (),
            ws::Message::Binary(bytes) => {
                let bytes = bytes.to_vec();
                let packet = ChatPacket::deserialize(bytes);

                match packet.packet_type {
                    proto::ChatPacketType::Close => {
                        ctx.close(None);
                        ctx.stop();
                    }
                    proto::ChatPacketType::Login => {
                        self.heartbeat = Instant::now();

                        let current_local = chrono::Local::now();
                        let time = current_local.format("%Y-%m-%d %H:%M:%S");

                        let time_and_tip = if let Some(name) = &self.name {
                            format!(
                                "[{}] ID_{} changed name from {} to {}",
                                time, self.id, name, packet.packet_message
                            )
                        } else {
                            format!(
                                "[{}] ID_{} set name to {}",
                                time, self.id, packet.packet_message
                            )
                        };

                        let pack = ChatPacket::new(proto::ChatPacketType::Login, time_and_tip);
                        self.name = Some(packet.packet_message.clone());

                        self.addr
                            .send(pack)
                            .into_actor(self)
                            .then(|res, act, ctx| {
                                match res {
                                    Ok(_res) => {
                                        log::debug!("{} logined", act.name.clone().unwrap());
                                    }
                                    // something is wrong with chat server
                                    _ => ctx.stop(),
                                }
                                fut::ready(())
                            })
                            .wait(ctx);
                    }
                    proto::ChatPacketType::Chat => {
                        self.heartbeat = Instant::now();

                        let current_local = chrono::Local::now();
                        let time = current_local.format("%Y-%m-%d %H:%M:%S");

                        let time_and_name = if let Some(name) = &self.name {
                            format!("[{}] {}: ", time, name)
                        } else {
                            format!("[{}] ID_{}: ", time, self.id)
                        };
                        let pack = ChatPacket::new(
                            proto::ChatPacketType::Chat,
                            time_and_name + packet.packet_message.as_str(),
                        );

                        self.addr
                            .send(pack)
                            .into_actor(self)
                            .then(|_res, _act, _ctx| fut::ready(()))
                            .wait(ctx);
                    }
                    _ => {
                        log::error!("unknown packet type: {:?}", packet.packet_type);
                        ctx.close(None);
                        ctx.stop();
                    }
                }
            }
            ws::Message::Close(reason) => {
                log::debug!("websocket client close: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                ctx.stop();
            }
            ws::Message::Nop => (),
        }
    }
}

/// Handler for Package message.
/// for notify bytes to client
impl Handler<ChatPacket> for WsSession {
    type Result = ();

    fn handle(&mut self, pkg: ChatPacket, ctx: &mut Self::Context) {
        println!("session handle send package");
        self.heartbeat = Instant::now();

        // send message
        let msg_bytes = pkg.serialize();
        ctx.binary(msg_bytes);
    }
}
