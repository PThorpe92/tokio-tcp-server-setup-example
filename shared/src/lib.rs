use bytes::{Bytes, BytesMut};
use rmp_serde::Deserializer;
use serde::{Deserialize, Serialize};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
pub type ClientId = usize;

pub const HEARTBEAT_INTERVAL: u64 = 5;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

pub type ClientStream = FramedRead<OwnedReadHalf, LengthDelimitedCodec>;
pub type ClientSink = FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>;

#[derive(Debug, Serialize, Deserialize)]
pub enum Msg {
    ClientHello,
    ClientHelloAck(ClientId), // client id
    // ...
    Heartbeat,
    MissedHeartbeat,
    GracefulDisconnect,
}

impl Msg {
    pub fn serialize(&self) -> Result<Bytes, rmp_serde::encode::Error> {
        let buff = rmp_serde::to_vec_named(&self)?;
        Ok(Bytes::from(buff))
    }
    pub fn deserialize(byte_seq: &BytesMut) -> Result<Self, rmp_serde::decode::Error> {
        let mut de = Deserializer::new(std::io::Cursor::new(byte_seq));
        Deserialize::deserialize(&mut de)
    }
}
