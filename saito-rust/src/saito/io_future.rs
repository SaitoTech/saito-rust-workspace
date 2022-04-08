use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use log::{debug, trace};

use crate::saito::rust_io_handler::{FutureState, SHARED_CONTEXT};

pub struct IoFuture {
    pub event_id: u64,
}

impl Future for IoFuture {
    type Output = Result<FutureState, std::io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        debug!("checking result for event : {:?}", self.event_id,);
        let mut context = SHARED_CONTEXT.lock().unwrap();

        debug!("lock acquired");
        let result = context.future_states.remove(&self.event_id);
        if result.is_none() {
            context
                .future_wakers
                .insert(self.event_id, cx.waker().clone());

            // TODO : HACK : thread is busy waiting here. need to fix this
            // cx.waker().wake_by_ref();
            debug!("waiting for event : {:?}", self.event_id);
            return Poll::Pending;
        }
        debug!("event result found for : {:?}", self.event_id);
        Poll::Ready(Ok(result.unwrap()))
    }
}
