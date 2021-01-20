use std::{pin::Pin, task::Context, task::Poll};

use futures::{stream::Stream, StreamExt};

use nash_protocol::errors::Result;
use nash_protocol::protocol::subscriptions::SubscriptionResponse;
use nash_protocol::protocol::ResponseOrError;

use crate::Client;

impl Stream for Client {
    type Item = Result<ResponseOrError<SubscriptionResponse>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.global_subscription_receiver.poll_next_unpin(cx)
    }
}
