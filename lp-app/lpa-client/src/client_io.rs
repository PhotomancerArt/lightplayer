use async_trait::async_trait;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

/// Runtime-neutral I/O for the LightPlayer server protocol.
///
/// Core `LpClient` code uses this trait instead of depending on Tokio or
/// requiring `Send`. Host/native adapters can add those requirements at their
/// boundary.
#[async_trait(?Send)]
pub trait ClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError>;

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError>;

    async fn close(&mut self) -> Result<(), TransportError>;
}

#[async_trait(?Send)]
impl<T> ClientIo for Box<T>
where
    T: ClientIo + ?Sized,
{
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        (**self).send(msg).await
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        (**self).receive().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        (**self).close().await
    }
}
