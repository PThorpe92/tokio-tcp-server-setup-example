use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use shared::{ClientId, ClientSink, ClientStream, Msg, HEARTBEAT_INTERVAL};
use std::sync::{atomic::AtomicUsize, Arc};
use tokio::net::TcpListener;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

pub static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(100);

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let env = std::env::var("APP_ENV").unwrap_or("dev".to_string());
    let port = std::env::var("PORT").unwrap_or("8080".to_string());
    let log_level = std::env::var("LOG_LEVEL").unwrap_or("INFO".to_string());
    let level = match log_level.to_uppercase().as_str() {
        "DEBUG" => Level::DEBUG,
        "INFO" => Level::INFO,
        "WARN" => Level::WARN,
        "ERROR" => Level::ERROR,
        _ => Level::INFO,
    };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_line_number(true)
        .pretty()
        .with_ansi(env != "prod")
        .finish();
    info!("Starting server on port {}", port);
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    let span = tracing::span!(Level::INFO, "main_span");
    let _enter = span.enter();
    tokio::spawn(async move {
        handle_connection(port).await;
    });
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

async fn handle_connection(port: String) {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .expect("failed to bind to port");
    loop {
        if let Ok((stream, _)) = listener.accept().await {
            // one set of mpsc channels per client, one sender you can keep either in a static map of
            // clients or pass around to tasks that need to send to the client, and the receiver is
            // owned by the task that writes to the sink. you can think about this like you
            // internal messaging to and from the client, while a task you spawn on connection
            // each owns the actual sink/steam
            let (client_tx, client_rx) = tokio::sync::mpsc::channel::<Msg>(256);
            //
            // another set of mpsc channels for internal server state you can choose similarly.
            // likely the receiver would be owned by it's own task and that will get internal
            // msgs and process them into messages to send to the client tx.
            // but ultimately how you structure this is up to you
            let (server_state_tx, server_state_rx) = tokio::sync::mpsc::channel::<Msg>(256);

            let (read_half, write_half) = stream.into_split();
            let mut read_framed = FramedRead::new(
                read_half,
                LengthDelimitedCodec::builder()
                    .length_field_length(2)
                    .big_endian()
                    .new_codec(),
            );
            let mut write_framed = FramedWrite::new(
                write_half,
                LengthDelimitedCodec::builder()
                    .length_field_length(2)
                    .big_endian()
                    .new_codec(),
            );
            if let Some(Ok(msg)) = read_framed.next().await {
                if let Ok(Msg::ClientHello) = Msg::deserialize(&msg) {
                    info!("Received ClientHello");
                    let client_id = add_client(&client_tx);
                    let _ = write_framed
                        .send(
                            Msg::ClientHelloAck(client_id)
                                .serialize()
                                .unwrap_or_default(),
                        )
                        .await;
                    tokio::spawn(async move {
                        client_inbound(client_id, read_framed, server_state_tx).await;
                    });
                    tokio::spawn(async move {
                        // the clients receiving channel is owned by the task handling the writer
                        client_outbound(client_id, write_framed, client_rx).await;
                    });
                    // here you can spawn your tasks to kick off the remainder of your internal
                    // processing and logic, at least one will own the servers rx channel
                }
            } else {
                info!("Did not receive ClientHello");
                continue;
            }
        }
    }
}
pub struct Client {
    pub id: ClientId,
    pub tx: tokio::sync::mpsc::Sender<Msg>,
}

impl Client {
    pub fn new(id: ClientId, tx: &tokio::sync::mpsc::Sender<Msg>) -> Self {
        Self { id, tx: tx.clone() }
    }
}

fn add_client(tx: &tokio::sync::mpsc::Sender<Msg>) -> usize {
    let id = NEXT_CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let _client = Arc::new(Client::new(id, tx));
    // proceed with client into either static map, rwLock or other approach of your choice
    id
}

#[tracing::instrument(skip(reader, tx))]
async fn client_inbound(
    client_id: ClientId,
    mut reader: ClientStream,
    tx: tokio::sync::mpsc::Sender<Msg>,
) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(HEARTBEAT_INTERVAL));
    interval.reset_at(
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(HEARTBEAT_INTERVAL),
    );
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let _ = tx.send(Msg::MissedHeartbeat).await;
            }
            Some(Ok(msg)) = reader.next() => {
                match Msg::deserialize(&msg) {
                    Ok(Msg::Heartbeat) => {
                        info!("Client {} sent heartbeat", client_id);
                        interval.reset_at(tokio::time::Instant::now());
                    }
                    Ok(Msg::GracefulDisconnect) => {
                        info!("Client {} disconnected", client_id);
                    }
                    Ok(msg) => {
                          info!("Client {} sent message: {:?}", client_id, msg);
                    }
                    Err(e) => {
                         info!("Error deserializing message from client {}: {:?}", client_id, e);
                    }
                }
            }
        }
    }
}

#[tracing::instrument(skip(writer, rx))]
async fn client_outbound(
    client_id: ClientId,
    mut writer: ClientSink,
    mut rx: tokio::sync::mpsc::Receiver<Msg>,
) {
    loop {
        if let Some(msg) = rx.recv().await {
            if let Err(e) = writer.send(msg.serialize().unwrap_or(Bytes::new())).await {
                info!("Error sending message to client {}: {:?}", client_id, e);
            }
        }
    }
}
