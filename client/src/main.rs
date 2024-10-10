use dotenvy::dotenv;
use futures::{SinkExt, StreamExt};
use shared::{Msg, HEARTBEAT_INTERVAL};
use std::{io, time::Duration};
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::mpsc::{Receiver, Sender},
    time,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tracing::{debug, error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> io::Result<()> {
    let _ = dotenv();
    let port = std::env::var("PORT").unwrap_or("8080".to_string());
    init_logging();
    let _span = tracing::span!(tracing::Level::INFO, "client");
    let _guard = _span.enter();
    let stream = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
    let (read_half, write_half) = stream.into_split();
    let framed_write = FramedWrite::new(
        write_half,
        LengthDelimitedCodec::builder()
            .length_field_length(2)
            .big_endian()
            .new_codec(),
    );
    let framed_read = FramedRead::new(
        read_half,
        LengthDelimitedCodec::builder()
            .length_field_length(2)
            .big_endian()
            .new_codec(),
    );
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    tokio::spawn(incoming_frames(framed_read, tx.clone()));
    tokio::spawn(outgoing_frames(framed_write, rx));

    if tx.send(Msg::ClientHello).await.is_ok() {
        info!("Sent client hello");
    }
    tokio::signal::ctrl_c().await
}

fn init_logging() {
    let env = std::env::var("APP_ENV").unwrap_or("dev".to_string());
    let log_level = std::env::var("LOG_LEVEL").unwrap_or(match env.as_str() {
        "prod" | "production" => "INFO".to_string(),
        _ => "DEBUG".to_string(),
    });
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
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

// send heartbeat msg at specified interval in shared/src/lib.rs
async fn handle_heartbeat(tx: Sender<Msg>) {
    let mut interval = time::interval(Duration::from_secs(HEARTBEAT_INTERVAL));
    loop {
        interval.tick().await;
        let msg = Msg::Heartbeat;
        tx.send(msg).await.unwrap();
    }
}

type ClientStream = FramedRead<OwnedReadHalf, LengthDelimitedCodec>;
type ClientSink = FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>;

async fn incoming_frames(mut framed_read: ClientStream, tx: Sender<Msg>) {
    while let Some(msg) = framed_read.next().await {
        match msg {
            Ok(msg) => {
                let decoded = Msg::deserialize(&msg).expect("Failed to deserialize message");
                match decoded {
                    Msg::ClientHelloAck(id) => {
                        info!("Received client hello ack: new client id is {}", id);
                        let hb_tx = tx.clone();
                        tokio::spawn(handle_heartbeat(hb_tx));
                    }
                    _ => {
                        error!("Received unexpected message: {:?}", msg);
                    }
                }
            }
            Err(e) => {
                error!("Failed to read from socket: {:?}", e);
                break;
            }
        }
    }
}

async fn outgoing_frames(mut framed_write: ClientSink, mut rx: Receiver<Msg>) {
    let _span = tracing::span!(tracing::Level::INFO, "outgoing_frames");
    let _guard = _span.enter();
    while let Some(msg) = rx.recv().await {
        debug!("Message to send:\n{:#?}", msg);
        if let Ok(encoded) = msg.serialize() {
            info!("Sending message: {:?}", encoded);
            let _ = framed_write.send(encoded).await;
        }
    }
}
