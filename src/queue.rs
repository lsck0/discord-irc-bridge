use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub struct TimedVecQueue<T> {
    items: VecDeque<T>,
    next_dequeue_at: Instant,
    minimum_dequeue_delay: Duration,
}

impl<T> TimedVecQueue<T> {
    pub fn new(min_interval: Duration) -> Self {
        return Self {
            items: VecDeque::new(),
            next_dequeue_at: Instant::now(),
            minimum_dequeue_delay: min_interval,
        };
    }

    pub fn push(&mut self, item: T) {
        self.items.push_back(item);
    }

    pub fn requeue(&mut self, item: T) {
        self.items.push_front(item);
    }

    pub fn try_pop(&mut self) -> Option<T> {
        if self.items.is_empty() || Instant::now() < self.next_dequeue_at {
            return None;
        }
        let item = self.items.pop_front()?;
        self.next_dequeue_at = Instant::now() + self.minimum_dequeue_delay;
        return Some(item);
    }

    pub fn time_until_ready(&self) -> Option<Duration> {
        if self.items.is_empty() {
            return None;
        }
        return Some(self.next_dequeue_at.saturating_duration_since(Instant::now()));
    }

    pub async fn wait_until_ready(&self) {
        match self.time_until_ready() {
            None => std::future::pending().await,
            Some(wait) => tokio::time::sleep(wait).await,
        }
    }

    pub fn backoff(&mut self, delay: Duration) {
        let resume_at = Instant::now() + delay;
        if resume_at > self.next_dequeue_at {
            self.next_dequeue_at = resume_at;
        }
    }
}
