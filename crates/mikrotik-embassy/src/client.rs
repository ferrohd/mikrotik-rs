//! The producer side of the command channel.

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::Sender;

use mikrotik_proto::command::{Command, CommandBuilder, NoCmd};
use mikrotik_proto::tag::{Counter, Tag, TagSource};

/// A handle for submitting commands to a running [`run`](crate::run) task.
///
/// The command channel is one-way, so a command must carry its tag before it
/// enters it — there is no reply path to learn one afterwards. `Client` pairs
/// the sender with the counter that supplies them. See the crate docs for an
/// example.
///
/// Not `Sync`: its [`Counter`] is a `Cell`, because a target without
/// compare-and-swap cannot have an atomic. The [`Sender`] is `Copy`, so one
/// `Client` per task works — give them disjoint counters with
/// [`with_tags`](Client::with_tags).
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

    /// As [`new`](Self::new), with a caller-supplied counter, so two clients
    /// feeding one task cannot collide.
    #[must_use]
    pub const fn with_tags(tx: Sender<'ch, M, Command, N>, tags: Counter) -> Self {
        Self { tx, tags }
    }

    /// Begins a command with the next tag; the counterpart to the `std`-only
    /// `CommandBuilder::new`.
    pub fn build(&self) -> CommandBuilder<NoCmd> {
        CommandBuilder::with_tag(self.next_tag())
    }

    /// Submits a command, waiting for room.
    pub async fn send(&self, command: Command) {
        self.tx.send(command).await;
    }

    /// Submits a command without waiting.
    ///
    /// # Errors
    ///
    /// Returns the command unsent if the channel is full.
    pub fn try_send(&self, command: Command) -> Result<(), Command> {
        self.tx.try_send(command).map_err(|err| match err {
            embassy_sync::channel::TrySendError::Full(command) => command,
        })
    }

    /// Takes the next tag without building a command.
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
