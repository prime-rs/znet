#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
}

impl Message {
    #[inline]
    pub fn new(topic: &str, payload: Vec<u8>) -> Self {
        Message {
            topic: topic.to_string(),
            payload,
        }
    }
}
