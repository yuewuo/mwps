//! Dual Module with Priority Queue
//!
//! A serial implementation of the dual module with priority queue optimization
//!

use crate::util::*;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

#[derive(Debug)]
pub struct FutureEvent<T: Ord + PartialEq + Eq, E> {
    /// when the event will happen
    pub time: T,
    /// the event
    pub event: E,
}

impl<T: Ord + PartialEq + Eq, E> PartialEq for FutureEvent<T, E> {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl<T: Ord + PartialEq + Eq, E> Eq for FutureEvent<T, E> {}

impl<T: Ord + PartialEq + Eq, E> Ord for FutureEvent<T, E> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time.cmp(&other.time)
    }
}

impl<T: Ord + PartialEq + Eq, E> PartialOrd for FutureEvent<T, E> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum Obstacle {
    Conflict { edge_index: EdgeIndex },
    ShrinkToZero { node_index: NodeIndex },
}

pub type FutureObstacle<T> = FutureEvent<T, Obstacle>;
pub type MinBinaryHeap<F> = BinaryHeap<Reverse<F>>;
pub type FutureObstacleQueue<T> = MinBinaryHeap<FutureObstacle<T>>;

pub trait FutureQueueMethods<T: Ord + PartialEq + Eq, E> {
    fn will_happen(&mut self, time: T, event: E);
    fn peek_event(&self) -> Option<(&T, &E)>;
    fn pop_event(&mut self) -> Option<(T, E)>;
}

impl<T: Ord + PartialEq + Eq, E> FutureQueueMethods<T, E> for MinBinaryHeap<FutureEvent<T, E>> {
    fn will_happen(&mut self, time: T, event: E) {
        self.push(Reverse(FutureEvent { time, event }))
    }
    fn peek_event(&self) -> Option<(&T, &E)> {
        self.peek().map(|future| (&future.0.time, &future.0.event))
    }
    fn pop_event(&mut self) -> Option<(T, E)> {
        self.pop().map(|future| (future.0.time, future.0.event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_module_pq_learn_priority_queue_1() {
        // cargo test dual_module_pq_learn_priority_queue_1 -- --nocapture
        let mut future_obstacle_queue = FutureObstacleQueue::<usize>::new();
        assert_eq!(0, future_obstacle_queue.len());
        macro_rules! ref_event {
            ($index:expr) => {
                Some((&$index, &Obstacle::Conflict { edge_index: $index }))
            };
        }
        macro_rules! value_event {
            ($index:expr) => {
                Some(($index, Obstacle::Conflict { edge_index: $index }))
            };
        }
        // test basic order
        future_obstacle_queue.will_happen(2, Obstacle::Conflict { edge_index: 2 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(3, Obstacle::Conflict { edge_index: 3 });
        assert_eq!(future_obstacle_queue.peek_event(), ref_event!(1));
        assert_eq!(future_obstacle_queue.peek_event(), ref_event!(1));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.peek_event(), ref_event!(2));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(2));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(3));
        assert_eq!(future_obstacle_queue.peek_event(), None);
        // test duplicate elements, the queue must be able to hold all the duplicate events
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.peek_event(), None);
        // test order of events at the same time
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 2 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 3 });
        let mut events = vec![];
        while let Some((time, event)) = future_obstacle_queue.pop_event() {
            assert_eq!(time, 1);
            events.push(event);
        }
        assert_eq!(events.len(), 3);
        println!("events: {events:?}");
    }
}
