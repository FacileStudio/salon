//! Server-side reads and writes of a room's document, so open members see them live, and the
//! notices the app sends its rooms.

use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use uuid::Uuid;
use yrs::sync::{Message, SyncMessage};

use crate::calls::restarting;
use crate::command::{AnswerFn, Command, EditFn, Job, Outcome};
use crate::error::{Fault, Kind};
use crate::hooks::{Due, Host};
use crate::hub::Hub;
use crate::room::Room;
use crate::sync::encode;

/// A server-side change: `edit` runs on the document in the room and answers the v1 update to
/// relay (`None` when it changed nothing); `answer` reads the document once the change is
/// saved, with whether it is stored. A refused edit must leave the document as it was.
pub struct Change<E, A> {
    /// How the change is saved before the answer.
    pub due: Due,
    /// Who made it, for the store.
    pub author: Option<i64>,
    /// The change itself.
    pub edit: E,
    /// What the caller gets back.
    pub answer: A,
}

fn guarded<S: crate::Space>(space: &mut S, edit: EditFn<S>) -> Result<Option<Vec<u8>>, ()> {
    let run = std::panic::AssertUnwindSafe(|| edit(&mut *space));
    std::panic::catch_unwind(run).unwrap_or_else(|_| space.recover().map_err(drop).and(Err(())))
}

impl<H: Host> Room<H> {
    /// Runs a server-side write: validated, relayed to every member, saved, then answered.
    pub(crate) async fn write(&mut self, job: Job<H::Space>) {
        let Job { edit, answer, due, author, .. } = job;
        let Some(applied) = self.lend(move |space, _| guarded(space, edit)).await else {
            self.end(1011, "room failed");
            let fault = Fault::new(Kind::Internal, format!("the {} edit did not finish", self.policy.noun));
            return answer(None, Outcome::Failed(fault));
        };
        let update = match applied {
            Err(()) => return answer(Some(&self.space), Outcome::Refused),
            Ok(None) => return answer(Some(&self.space), Outcome::Done(true)),
            Ok(Some(update)) => update,
        };
        self.broadcast(None, &encode(Message::Sync(SyncMessage::Update(update.clone()))));
        self.accepted(update.len(), author);
        self.settle(due).await;
        match self.ended {
            true => answer(None, Outcome::Ended),
            false => answer(Some(&self.space), Outcome::Done(!self.owed.unflushed)),
        }
    }
}

fn answering<H: Host, T: Send + 'static>(
    noun: &'static str,
    answer: impl FnOnce(&H::Space, bool) -> T + Send + 'static,
    slot: Arc<Mutex<Option<H::Error>>>,
    reply: oneshot::Sender<Result<T, H::Error>>,
) -> AnswerFn<H::Space> {
    Box::new(move |space, outcome| {
        let refused = || slot.lock().unwrap_or_else(|p| p.into_inner()).take();
        let internal = || Fault::new(Kind::Internal, format!("the {noun} edit failed"));
        let result = match (space, outcome) {
            (Some(space), Outcome::Done(saved)) => Ok(answer(space, saved)),
            (_, Outcome::Refused) => Err(refused().unwrap_or_else(|| H::fault(internal()))),
            (_, Outcome::Ended) => Err(H::fault(Fault::new(Kind::NotFound, format!("{noun} not found")))),
            (_, Outcome::Failed(fault)) => Err(H::fault(fault)),
            (None, Outcome::Done(_)) => Err(H::fault(internal())),
        };
        _ = reply.send(result);
    })
}

impl<H: Host> Hub<H> {
    /// What `read` answers from the live document of room `key`, loading it when needed.
    pub async fn read<T: Send + 'static>(
        &self,
        host: &H,
        key: Uuid,
        read: impl Fn(&H::Space) -> T + Clone + Send + 'static,
    ) -> Result<T, H::Error> {
        let make = |reply: oneshot::Sender<Result<T, Fault>>| {
            let read = read.clone();
            Command::Read(Box::new(move |space: Result<&H::Space, Fault>| _ = reply.send(space.map(read))))
        };
        self.ask(host, key, make).await.map_err(H::fault)
    }

    /// Runs `change` inside room `key`: validated like a member's update, relayed to the open
    /// members and saved before the answer. A room that unloads first gets it again, three times.
    pub async fn write<T, E, A>(&self, host: &H, key: Uuid, change: Change<E, A>) -> Result<T, H::Error>
    where
        T: Send + 'static,
        E: FnOnce(&mut H::Space) -> Result<Option<Vec<u8>>, H::Error> + Send + 'static,
        A: FnOnce(&H::Space, bool) -> T + Send + 'static,
    {
        let (noun, slot) = (host.rooms().1.noun, Arc::new(Mutex::new(None)));
        let (reply, answered) = oneshot::channel();
        let kept = slot.clone();
        let edit = change.edit;
        let edit: EditFn<H::Space> =
            Box::new(move |space| edit(space).map_err(|e| *kept.lock().unwrap_or_else(|p| p.into_inner()) = Some(e)));
        let answer = answering::<H, T>(noun, change.answer, slot, reply);
        let mut job = Some(Job { edit, answer, due: change.due, author: change.author, bounce: None });
        for _ in 0..3 {
            let Some(next) = job.take() else { break };
            job = self.deliver(host, key, next).await;
        }
        drop(job);
        answered.await.unwrap_or_else(|_| Err(H::fault(restarting(noun))))
    }

    async fn deliver(&self, host: &H, key: Uuid, mut job: Job<H::Space>) -> Option<Job<H::Space>> {
        let (bounce, back) = oneshot::channel();
        job.bounce = Some(bounce);
        if let Err(tokio::sync::mpsc::error::SendError(command)) =
            self.sender(host, key).send(Command::Write(job)).await
        {
            let Command::Write(job) = command else { return None };
            return Some(job);
        }
        back.await.ok()
    }
}
