use futures::{StreamExt, stream::Stream};
use std::{pin::Pin, task::Context, task::Poll};
use super::client::Client;
use super::absinthe::AbsintheWSResponse;


use nash_protocol::protocol::subscriptions::SubscriptionResponse;


impl Stream for Client {
    type Item=SubscriptionResponse;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.global_subscription_receiver.poll_next_unpin(cx)
    }

}