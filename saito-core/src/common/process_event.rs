use std::time::Duration;

use async_trait::async_trait;

use crate::common::command::{GlobalEvent, NetworkEvent};

/// Event Processing trait for the controllers. Handles both events from actions and timer
#[async_trait]
pub trait ProcessEvent<T>
where
    T: Send,
{
    /// Process an event from the global channel
    /// TODO : to be removed if not required
    ///
    /// # Arguments
    ///
    /// * `event`:
    ///
    /// returns: Option<()>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn process_global_event(&mut self, event: GlobalEvent) -> Option<()>;

    /// Processes an event coming from other peers via network controller
    ///
    /// # Arguments
    ///
    /// * `event`:
    ///
    /// returns: Option<()>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn process_network_event(&mut self, event: NetworkEvent) -> Option<()>;
    /// Triggered with each timer tick. duration will vary due to other processing tasks in the same thread.
    ///
    /// # Arguments
    ///
    /// * `duration`:
    ///
    /// returns: Option<()>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn process_timer_event(&mut self, duration: Duration) -> Option<()>;
    /// Processes the incoming events from other threads/controllers.
    ///
    /// # Arguments
    ///
    /// * `event`:
    ///
    /// returns: Option<()>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn process_event(&mut self, event: T) -> Option<()>;
    async fn on_init(&mut self);
}
