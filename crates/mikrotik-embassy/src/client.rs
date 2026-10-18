//! The producer side of the command channel.

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::Sender;

use mikrotik_proto::command::{Command, CommandBuilder, NoCmd};
use mikrotik_proto::tag::{Counter, Tag, TagSource};

/// A handle for submitting commands to a running [`run`](crate::run) task.
///
/// [`run`](crate::run) takes a one-way `Receiver<Command>`, so a command has to carry its
/// tag before it enters the channel — there is no reply path to learn one
/// afterwards. `Client` pairs the sender with the counter that supplies them,
/// which is the whole of what every consumer would otherwise write by hand.
///
/// # Example
///
/// ```rust,ignore
/// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
/// use embassy_sync::channel::Channel;
/// use mikrotik_embassy::Client;
/// use mikrotik_proto::command::Command;
/// use mikrotik_proto::connection::Event;
///
/// static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
/// static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
///
/// #[embassy_executor::task]
/// async fn poller() {
///     let client = Client::new(CMD.sender());
///
///     // Build, remember the tag, send.
///     let cmd = client.build().command("/system/resource/print").build();
///     let tag = cmd.tag;
///     client.send(cmd).await;
///
///     // Events arrive on one channel for every command, so filter by tag.
///     loop {
///         let event = EVT.receive().await;
///         if event.tag() == Some(tag) {
///             // ...
///         }
///     }
/// }
/// ```
///
/// # Sharing
///
/// `Client` is deliberately **not** `Sync`: its counter is a
/// [`Counter`](mikrotik_proto::tag::Counter), which is a `Cell`, because a
/// Cortex-M0+ has no compare-and-swap. One `Client` per task, or wrap it in a
/// mutex of your choosing. The underlying [`Sender`] is `Copy`, so several
/// tasks can hold their own `Client` over the same channel — each gets tags
/// from its own counter, so give them disjoint starting points with
/// [`Client::with_tags`] if you do.
pub struct Client<'ch, M: RawMutex, const N: usize> {
    tx: Sender<'ch, M, Command, N>,
    tags: Counter,
}

impl<'ch, M: RawMutex, const N: usize> Client<'ch, M, N> {
    /// Wrap the sending half of a command channel.
    #[must_use]
    pub const fn new(tx: Sender<'ch, M, Command, N>) -> Self {
        Self { tx, tags: Counter::new() }
    }

    /// As [`new`](Self::new), with a caller-supplied tag counter.
    ///
    /// Use this when more than one `Client` feeds the same [`run`](crate::run)
    /// task, so their tags cannot collide.
    #[must_use]
    pub const fn with_tags(tx: Sender<'ch, M, Command, N>, tags: Counter) -> Self {
        Self { tx, tags }
    }

    /// Begin a command, taking the next tag.
    ///
    /// Equivalent to `CommandBuilder::with_tag(client.next_tag())`, and the
    /// counterpart to the `std`-only `CommandBuilder::new`.
    pub fn build(&self) -> CommandBuilder<NoCmd> {
        CommandBuilder::with_tag(self.next_tag())
    }

    /// Submit a command, waiting for room in the channel.
    pub async fn send(&self, command: Command) {
        self.tx.send(command).await;
    }

    /// Submit a command, returning it if the channel is full.
    ///
    /// # Errors
    ///
    /// Returns the command unsent when the channel has no free slot.
    pub fn try_send(&self, command: Command) -> Result<(), Command> {
        self.tx.try_send(command).map_err(|err| match err {
            embassy_sync::channel::TrySendError::Full(command) => command,
        })
    }

    /// Take the next tag without building a command.
    #[must_use]
    pub fn next_tag(&self) -> Tag {
        self.tags.next_tag()
    }
}

impl<M: RawMutex, const N: usize> TagSource for Client<'_, M, N> {
    fn next_tag(&self) -> Tag {
        self.tags.next_tag()
    }
}

impl<M: RawMutex, const N: usize> core::fmt::Debug for Client<'_, M, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}
