use std::vec;

use color_eyre::{eyre::eyre, Result};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    pub topic: String,
    pub origin: u128,
    pub payload: Vec<u8>,
}

impl Message {
    #[inline]
    pub fn new(topic: &str, payload: Vec<u8>) -> Self {
        Message {
            topic: topic.to_string(),
            origin: 0,
            payload,
        }
    }

    #[inline]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = vec![];
        buf.extend_from_slice(&self.origin.to_be_bytes());
        if !self.topic.is_empty() {
            buf.extend_from_slice(&(self.topic.len() as u32).to_be_bytes());
            buf.extend_from_slice(self.topic.as_bytes());
        } else {
            buf.extend_from_slice(&[0, 0, 0, 0]);
        }
        buf.extend_from_slice(&self.payload);
        buf
    }

    #[inline]
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 16 {
            return Err(eyre!("invalid message"));
        }
        let origin = u128::from_be_bytes(bytes[0..16].try_into()?);
        let mut topic = String::new();
        let mut payload = Vec::new();
        let mut cursor = 16;
        if bytes.len() > cursor {
            let len = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into()?);
            cursor += 4;
            topic = String::from_utf8(bytes[cursor..cursor + len as usize].to_vec())?;
            cursor += len as usize;
            payload = bytes[cursor..].to_vec();
        }
        Ok(Message {
            topic,
            origin,
            payload,
        })
    }
}

#[test]
fn test_message_encode_decode() {
    let msg = Message::new("test", vec![1, 2, 3, 4, 5]);
    let bytes = msg.encode();
    println!("{:?}", bytes);
    let msg2 = Message::decode(&bytes).unwrap();
    assert_eq!(msg, msg2);
}
