use futures::{
  Poll, Stream,
  task::{Context, Waker},
};
use lapin_async::consumer::ConsumerSubscriber;
use log::trace;
use parking_lot::Mutex;

use std::{
  collections::VecDeque,
  pin::Pin,
  sync::Arc,
};

use crate::{
  message::Delivery,
  types::ShortString,
};

#[derive(Clone, Debug)]
pub struct ConsumerSub {
  inner: Arc<Mutex<ConsumerInner>>,
}

impl ConsumerSubscriber for ConsumerSub {
  fn new_delivery(&self, delivery: Delivery) {
    trace!("new_delivery;");
    let mut inner = self.inner.lock();
    inner.deliveries.push_back(delivery);
    if let Some(task) = inner.task.as_ref() {
      task.wake_by_ref();
    }
  }
  fn drop_prefetched_messages(&self) {
    trace!("drop_prefetched_messages;");
    let mut inner = self.inner.lock();
    inner.deliveries.clear();
  }
  fn cancel(&self) {
    trace!("cancel;");
    let mut inner = self.inner.lock();
    inner.deliveries.clear();
    inner.canceled = true;
    inner.task.take();
  }
}

#[derive(Clone)]
pub struct Consumer {
  inner:        Arc<Mutex<ConsumerInner>>,
  channel_id:   u16,
  queue:        ShortString,
  consumer_tag: ShortString,
}

#[derive(Debug)]
struct ConsumerInner {
  deliveries: VecDeque<Delivery>,
  task:       Option<Waker>,
  canceled:   bool,
}

impl Default for ConsumerInner {
  fn default() -> Self {
    Self {
      deliveries: VecDeque::new(),
      task:       None,
      canceled:   false,
    }
  }
}

impl Consumer {
  pub fn new(channel_id: u16, queue: ShortString, consumer_tag: ShortString) -> Consumer {
    Consumer {
      inner: Arc::new(Mutex::new(ConsumerInner::default())),
      channel_id,
      queue,
      consumer_tag,
    }
  }

  pub fn update_consumer_tag(&mut self, consumer_tag: ShortString) {
    self.consumer_tag = consumer_tag;
  }

  pub fn subscriber(&self) -> ConsumerSub {
    ConsumerSub { inner: self.inner.clone() }
  }
}

impl Stream for Consumer {
  type Item = Delivery;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    trace!("consumer poll; consumer_tag={:?} polling transport", self.consumer_tag);
    let mut inner = self.inner.lock();
    trace!("consumer poll; consumer_tag={:?} acquired inner lock", self.consumer_tag);
    if inner.task.is_none() {
      inner.task = Some(cx.waker().clone());
    }
    if let Some(delivery) = inner.deliveries.pop_front() {
      trace!("delivery; consumer_tag={:?} delivery_tag={:?}", self.consumer_tag, delivery.delivery_tag);
      Poll::Ready(Some(delivery))
    } else if inner.canceled {
      trace!("consumer canceled; consumer_tag={:?}", self.consumer_tag);
      Poll::Ready(None)
    } else {
      trace!("delivery; consumer_tag={:?} status=NotReady", self.consumer_tag);
      Poll::Pending
    }
  }
}
