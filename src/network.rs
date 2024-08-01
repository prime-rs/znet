use std::{
    borrow::Cow,
    collections::HashMap,
    io::Read,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use color_eyre::{eyre::eyre, Result};
use flume::Sender;
use indexmap::IndexSet;
use parking_lot::RwLock;
use zenoh::{
    liveliness::LivelinessToken,
    prelude::*,
    qos::{CongestionControl, Priority},
    query::{Query, QueryTarget},
    sample::{Locality, Sample, SampleKind},
    Session,
};
use zenoh_ext::SubscriberBuilderExt;

use crate::protocol::Message;

pub type NetworkConfig = zenoh::config::Config;
pub type NetworkMode = zenoh::config::WhatAmI;
pub type Callback = Box<dyn FnMut(Message) + Send + Sync + 'static>;
pub type CallbackWithReply = Box<dyn FnMut(Message) -> Message + Send + Sync + 'static>;

pub struct Subscriber {
    pub topic: String,
    pub callback: Callback,
}

impl Subscriber {
    pub fn new<C>(topic: &str, callback: C) -> Self
    where
        C: FnMut(Message) + Send + Sync + 'static,
    {
        Self {
            topic: topic.to_owned(),
            callback: Box::new(callback),
        }
    }

    pub(crate) fn topic(&self) -> String {
        self.topic.to_string()
    }

    pub(crate) fn callback_mut(mut self) -> Box<dyn FnMut(Sample) + Send + Sync + 'static> {
        Box::new(move |sample: Sample| {
            let net_msg = Message::decode(
                &sample
                    .payload()
                    .deserialize::<Cow<[u8]>>()
                    .unwrap_or_default(),
            )
            .unwrap_or(Message::new(sample.key_expr(), vec![]));
            (self.callback)(net_msg);
        })
    }
}

pub struct Queryable {
    pub topic: String,
    pub callback: CallbackWithReply,
}

impl Queryable {
    pub fn new<C>(topic: &str, callback: C) -> Self
    where
        C: FnMut(Message) -> Message + Send + Sync + 'static,
    {
        Self {
            topic: topic.to_owned(),
            callback: Box::new(callback),
        }
    }

    pub(crate) fn topic(&self) -> String {
        self.topic.to_string()
    }

    pub(crate) fn callback_mut(mut self) -> Box<dyn FnMut(Query) + Send + Sync + 'static> {
        Box::new(move |quary: Query| {
            let net_msg = Message::decode(
                &quary
                    .payload()
                    .map(|v| v.deserialize::<Cow<[u8]>>().unwrap_or_default())
                    .unwrap_or_default(),
            )
            .unwrap_or(Message::new(quary.key_expr(), vec![]));
            let reply_msg = (self.callback)(net_msg);
            tokio::spawn(async move {
                quary
                    .reply(quary.key_expr().clone(), reply_msg.encode())
                    .await
                    .map_err(|e| eyre!("reply failed: {e}"))
                    .ok();
            });
        })
    }
}

#[derive(Clone)]
pub struct Network {
    put_sender: Sender<Message>,
    get_sender: Sender<(Message, Callback)>,
    _session: Arc<Session>,
    _queryables: Arc<Vec<zenoh::query::Queryable<'static, ()>>>,
    _subscribers: Arc<Vec<zenoh::pubsub::Subscriber<'static, ()>>>,
    _liveliness_token: Arc<LivelinessToken<'static>>,
    _liveliness_subscriber: Arc<zenoh_ext::FetchingSubscriber<'static, ()>>,
}

impl Network {
    pub async fn serve(
        config: NetworkConfig,
        address: [u8; 16],
        subscribers: Vec<Subscriber>,
        queryables: Vec<Queryable>,
    ) -> Result<Self> {
        debug!("net starting: {config:#?}");

        let self_zid = *config.id();

        // open zenoh session
        let session = Arc::new(
            zenoh::open(config)
                .await
                .map_err(|e| eyre!("session open failed: {e}"))?,
        );

        let zid_list = Arc::new(RwLock::new(IndexSet::new()));
        {
            // zid_list[0] is self
            zid_list.write().insert(self_zid.to_string());
        }

        // liveliness declaring
        let liveliness_prefix = "liveliness/";
        let _liveliness_token = session
            .liveliness()
            .declare_token(&format!("{}{}", liveliness_prefix, self_zid))
            .await
            .map_err(|e| eyre!("liveliness declare failed: {e}"))?;

        // liveliness subscription
        let zid_list_c = zid_list.clone();
        let _liveliness_subscriber = session
            .liveliness()
            .declare_subscriber(format!("{}**", liveliness_prefix))
            .querying()
            .callback(move |sample| {
                if let Some(zid) = sample.key_expr().as_str().strip_prefix(liveliness_prefix) {
                    match sample.kind() {
                        SampleKind::Put => {
                            zid_list_c.write().insert(zid.to_owned());
                            info!("[Peer connected]: new alive token ({})", zid);
                        }
                        SampleKind::Delete => {
                            if !self_zid.to_string().eq(zid) {
                                zid_list_c.write().shift_remove(zid);
                            }
                            info!("[Peer offline]: dropped token ({})", zid);
                        }
                    }
                }
            })
            .await
            .map_err(|e| eyre!("liveliness subscriber declare failed: {e}"))?;

        // subscribers
        let mut _subscribers = vec![];
        for subscriber in subscribers {
            info!("{}/{}", subscriber.topic(), self_zid);
            _subscribers.push(
                session
                    .declare_subscriber(format!("{}/{}", subscriber.topic(), self_zid))
                    .callback_mut(subscriber.callback_mut())
                    .best_effort()
                    .await
                    .map_err(|e| eyre!("subscriber declare failed: {e}"))?,
            );
        }

        let mut _queryables = vec![];

        // queryables
        for queryable in queryables {
            info!("{}/{}", queryable.topic(), self_zid);
            _queryables.push(
                session
                    .declare_queryable(format!("{}/{}", queryable.topic(), self_zid))
                    .callback_mut(queryable.callback_mut())
                    .await
                    .map_err(|e| eyre!("subscriber declare failed: {e}"))?,
            );
        }

        let last_sent_zid_index = Arc::new(AtomicUsize::new(0));
        let (put_msg_tx, put_msg_rv) = flume::bounded::<Message>(1024 * 8);
        let (get_msg_tx, get_msg_rv) = flume::bounded::<(Message, Callback)>(1024 * 8);

        // put
        let zid_list_c = zid_list.clone();
        let session_c = session.clone();
        let last_sent_zid_index_c = last_sent_zid_index.clone();
        let put_msg_rv = put_msg_rv.clone();
        tokio::spawn(async move {
            let mut pulishers: HashMap<String, zenoh::pubsub::Publisher<'static>> = HashMap::new();
            while let Ok(net_msg) = put_msg_rv.recv_async().await {
                let Ok((serialized_msg, topic)) =
                    dispatch_msg(net_msg, address, &zid_list_c, &last_sent_zid_index_c)
                else {
                    continue;
                };

                // publish
                if let Some(publisher) = pulishers.get(&topic) {
                    publisher
                        .put(serialized_msg)
                        .await
                        .map_err(|e| error!("put msg failed: {e}"))
                        .ok();
                } else if let Ok(publisher) = session_c
                    .declare_publisher(topic.clone())
                    .allowed_destination(Locality::Remote)
                    .congestion_control(CongestionControl::Block)
                    .priority(Priority::RealTime)
                    .await
                    .map_err(|e| error!("publisher declare failed: {e}"))
                {
                    publisher
                        .put(serialized_msg)
                        .await
                        .map_err(|e| error!("put msg failed: {e}"))
                        .ok();
                    pulishers.insert(topic, publisher);
                }
            }
        });

        // get
        let zid_list_c = zid_list.clone();
        let session_c = session.clone();
        let last_sent_zid_index_c = last_sent_zid_index.clone();
        let get_msg_rv = get_msg_rv.clone();
        tokio::spawn(async move {
            while let Ok((net_msg, mut callback)) = get_msg_rv.recv_async().await {
                let Ok((serialized_msg, topic)) =
                    dispatch_msg(net_msg, address, &zid_list_c, &last_sent_zid_index_c)
                else {
                    continue;
                };

                if let Ok(replies) = session_c
                    .get(&topic)
                    .payload(serialized_msg)
                    .target(QueryTarget::BestMatching)
                    .timeout(Duration::from_secs(5))
                    .await
                {
                    if let Ok(reply) = replies.recv_async().await {
                        if let Ok(sample) = reply.result() {
                            let mut buf = vec![0; sample.payload().len()];
                            sample.payload().reader().read_exact(&mut buf).ok();
                            if let Ok(reply_msg) = Message::decode(&buf) {
                                (callback)(reply_msg);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            _session: session,
            put_sender: put_msg_tx,
            get_sender: get_msg_tx,
            _subscribers: Arc::new(_subscribers),
            _queryables: Arc::new(_queryables),
            _liveliness_token: Arc::new(_liveliness_token),
            _liveliness_subscriber: Arc::new(_liveliness_subscriber),
        })
    }

    pub fn put_sender(&self) -> Sender<Message> {
        self.put_sender.clone()
    }

    pub fn get_sender(&self) -> Sender<(Message, Callback)> {
        self.get_sender.clone()
    }
}

fn dispatch_msg(
    mut net_msg: Message,
    address: [u8; 16],
    zid_list_c: &RwLock<IndexSet<String>>,
    last_sent_zid_index_c: &AtomicUsize,
) -> Result<(Vec<u8>, String)> {
    // set origin
    net_msg.origin = u128::from_le_bytes(address);
    let serialized_msg = net_msg.encode();
    // debug!("outbound msg: {:?}", &net_msg);

    let topic = if net_msg.topic.ends_with("/*") || net_msg.topic.ends_with("/**") {
        net_msg.topic
    } else {
        // pick zid
        let zid = {
            let zid_list = zid_list_c.read();
            let zid_list_len = zid_list.len();
            if zid_list_len < 2 {
                return Err(eyre!("no alive peer"));
            }
            let zid_index = std::cmp::max(
                (last_sent_zid_index_c.load(Ordering::Relaxed) + 1) % zid_list.len(),
                1,
            );
            last_sent_zid_index_c.store(zid_index, Ordering::Relaxed);
            zid_list[zid_index].clone()
        };
        format!("{}/{}", net_msg.topic, zid)
    };

    debug!("topic: {}", topic);
    Ok((serialized_msg, topic))
}
