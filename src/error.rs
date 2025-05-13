use thiserror::Error;

/// Project specific error types
#[derive(Error, Debug)]
pub enum CyclicGraphError<I> {
    /// The error was caused when new node inserting before the Input Node
    #[error("Cannot insert node before input node")]
    InsertBeforeInput,

    /// The error was caused when new node appending after the Output Node
    #[error("Cannot insert node after output node")]
    InsertAfterOutput,

    /// The error was caused when trying to remove the Input Node
    #[error("Cannot remove input node")]
    RemoveInput,

    /// The error was caused when trying to remove the Output Node
    #[error("Cannot remove output node")]
    RemoveOutput,

    /// The error was caused when trying get non-existing node by id
    #[error("Node not found by id: {0}")]
    NodeNotFoundById(I),

    /// The error was caused when trying add node with non-unique id
    #[error("Entered id `{0}` is not unique")]
    NonUniqueId(I),

    /// The error was caused at LinksProvider handler
    #[error("LinksProvider handler was caused error: {0}")]
    LinksProviderHandlerError(String),

    /// The error was caused at LinksAcceptor handler
    #[error("LinksAcceptor handler was caused error: {0}")]
    LinksAcceptorHandlerError(String),

    /// The error was caused when TryLockError raised
    #[error("TryLockError occurred")]
    TryLockError(#[from] tokio::sync::TryLockError),
}
