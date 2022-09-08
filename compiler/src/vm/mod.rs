mod builtin_functions;
mod channel;
pub mod context;
mod fiber;
mod heap;
pub mod tracer;
mod use_module;

use std::{marker::PhantomData, collections::{HashMap, VecDeque}, fmt};
pub use fiber::{Fiber, TearDownResult};
pub use heap::{Closure, Heap, Object, Pointer};
use itertools::Itertools;
use rand::seq::SliceRandom;
use tracing::{info, warn};
use crate::vm::heap::Struct;
use self::{heap::{ChannelId, SendPort}, channel::{ChannelBuf, Packet}, context::Context, tracer::Tracer};

/// A VM represents a Candy program that thinks it's currently running. Because
/// VMs are first-class Rust structs, they enable other code to store "freezed"
/// programs and to remain in control about when and for how long code runs.
#[derive(Clone)]
pub struct Vm {
    fibers: HashMap<FiberId, FiberTree>,
    root_fiber: FiberId,
    
    channels: HashMap<ChannelId, Channel>,
    pub external_operations: HashMap<ChannelId, Vec<Operation>>,

    fiber_id_generator: IdGenerator<FiberId>,
    channel_id_generator: IdGenerator<ChannelId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FiberId(usize);

#[derive(Clone)]
enum FiberTree {
    /// This tree is currently focused on running a single fiber.
    SingleFiber {
        fiber: Fiber,
        parent_nursery: Option<ChannelId>,
    },

    /// The fiber of this tree entered a `core.parallel` scope so that it's now
    /// paused and waits for the parallel scope to end. Instead of the main
    /// former single fiber, the tree now runs the closure passed to
    /// `core.parallel` as well as any other spawned children.
    ParallelSection {
        paused_tree: Box<FiberTree>,
        nursery: ChannelId,
    },
}

#[derive(Clone)]
enum Channel {
    Internal(InternalChannel),
    External(ChannelId),
    Nursery { parent: FiberId, children: Vec<Child> },
}
#[derive(Clone, Debug)]
struct InternalChannel {
    buffer: ChannelBuf,
    pending_sends: VecDeque<(Option<FiberId>, Packet)>,
    pending_receives: VecDeque<Option<FiberId>>,
}
#[derive(Clone)]
struct Child {
    fiber: FiberId,
    return_channel: ChannelId,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct OperationId(usize);

#[derive(Clone)]
pub struct Operation {
    performing_fiber: Option<FiberId>,
    kind: OperationKind,
}
#[derive(Clone, Debug)]
pub enum OperationKind {
    Send { packet: Packet },
    Receive,
}

#[derive(Clone, Debug)]
pub enum Status {
    Running,
    WaitingForOperations,
    Done,
    Panicked { reason: String },
}

impl Vm {
    fn new_with_fiber(fiber: Fiber) -> Self {
        let fiber = FiberTree::SingleFiber { fiber, parent_nursery: None };
        let mut fiber_id_generator = IdGenerator::start_at(0);
        let root_fiber_id = fiber_id_generator.generate();
        Self {
            channels: Default::default(),
            fibers: [(root_fiber_id, fiber)].into_iter().collect(),
            root_fiber: root_fiber_id,
            external_operations: Default::default(),
            channel_id_generator: IdGenerator::start_at(0),
            fiber_id_generator,
        }
    }
    pub fn new_for_running_closure(heap: Heap, closure: Pointer, arguments: &[Pointer]) -> Self {
        Self::new_with_fiber(Fiber::new_for_running_closure(heap, closure, arguments))
    }
    pub fn new_for_running_module_closure(closure: Closure) -> Self {
        Self::new_with_fiber(Fiber::new_for_running_module_closure(closure))
    }
    pub fn tear_down(mut self) -> TearDownResult {
        let fiber = self.fibers.remove(&self.root_fiber).unwrap().into_single_fiber().unwrap();
        fiber.tear_down()
    }

    pub fn status(&self) -> Status {
        self.status_of(self.root_fiber)
    }
    fn status_of(&self, fiber: FiberId) -> Status {
        match &self.fibers[&fiber] {
            FiberTree::SingleFiber { fiber, .. } => match &fiber.status {
                fiber::Status::Running => Status::Running,
                fiber::Status::Sending { .. } |
                fiber::Status::Receiving { .. } => Status::WaitingForOperations,
                fiber::Status::CreatingChannel { .. } |
                fiber::Status::InParallelScope { .. } => unreachable!(),
                fiber::Status::Done => Status::Done,
                fiber::Status::Panicked { reason } => Status::Panicked { reason: reason.clone() },
            },
            FiberTree::ParallelSection { nursery, .. } => {
                let children = self.channels[nursery].as_nursery_children().unwrap();
                if children.is_empty() {
                    return Status::Done;
                }
                for child in children {
                    match self.status_of(child.fiber) {
                        Status::Running => return Status::Running,
                        Status::WaitingForOperations => {},
                        Status::Done => continue,
                        Status::Panicked { reason } => return Status::Panicked { reason },
                    };
                }
                Status::WaitingForOperations
            },
        }
    }
    fn is_running(&self) -> bool {
        matches!(self.status(), Status::Running)
    }
    fn is_finished(&self) -> bool {
        matches!(self.status(), Status::Done | Status::Panicked { .. })
    }

    pub fn fiber(&self) -> &Fiber { // TODO: Remove before merging the PR
        todo!()
    }
    pub fn cloned_tracer(&self) -> Tracer {
        self.fiber().tracer.clone()
    }

    pub fn run<C: Context>(&mut self, context: &mut C) {
        assert!(self.is_running(), "Called Vm::run on a VM that is not ready to run.");

        let mut fiber_id = self.root_fiber;
        let fiber = loop {
            match self.fibers.get_mut(&fiber_id).unwrap() {
                FiberTree::SingleFiber { fiber, .. } => break fiber,
                FiberTree::ParallelSection { nursery, .. } => {
                    let children = self.channels.get_mut(&nursery).unwrap().as_nursery_children_mut().unwrap();
                    fiber_id = children
                        .choose(&mut rand::thread_rng()).unwrap().fiber;
                },
            }
        };

        if !matches!(fiber.status(), fiber::Status::Running) {
            return;
        }

        // TODO: Limit context.
        fiber.run(context);

        let is_finished = match fiber.status() {
            fiber::Status::Running => false,
            fiber::Status::CreatingChannel { capacity } => {
                let channel_id = self.channel_id_generator.generate();
                self.channels.insert(channel_id, Channel::Internal(InternalChannel { buffer: ChannelBuf::new(capacity), pending_sends: Default::default(), pending_receives: Default::default() }));
                fiber.complete_channel_create(channel_id);
                false
            },
            fiber::Status::Sending { channel, packet } => {
                self.send_to_channel(Some(fiber_id), channel, packet);
                false
            }
            fiber::Status::Receiving { channel } => {
                self.receive_from_channel(Some(fiber_id), channel);
                false
            }
            fiber::Status::InParallelScope { body, return_channel } => {
                let nursery_id = self.channel_id_generator.generate();

                let child_id = {
                    let mut heap = Heap::default();
                    let body = fiber.heap.clone_single_to_other_heap(&mut heap, body);
                    let nursery_send_port = heap.create_send_port(nursery_id);
                    let id = self.fiber_id_generator.generate();
                    self.fibers.insert(id, FiberTree::SingleFiber { fiber: Fiber::new_for_running_closure(heap, body, &[nursery_send_port]), parent_nursery: Some(nursery_id) });
                    id
                };

                // TODO: Make it so that the initial fiber doesn't need a return channel.
                let children = vec![Child {
                    fiber: child_id,
                    return_channel: return_channel,
                }];
                self.channels.insert(nursery_id, Channel::Nursery { parent: fiber_id, children });

                let paused_tree = self.fibers.remove(&fiber_id).unwrap();
                self.fibers.insert(fiber_id, FiberTree::ParallelSection { paused_tree: Box::new(paused_tree), nursery: nursery_id });

                // self.fibers.entry(fiber_id).and_modify(|fiber_tree| {
                //     let paused_main_fiber = match original {}
                //     FiberTree::ParallelSection { paused_main_fiber, nursery: nursery_id }
                // });

                false
            },
            fiber::Status::Done => {
                info!("A fiber is done.");
                true
            },
            fiber::Status::Panicked { reason } => {
                warn!("A fiber panicked because {reason}.");
                true
            },
        };
        
        if is_finished && fiber_id != self.root_fiber {
            let fiber = self.fibers.remove(&fiber_id).unwrap();
            let (fiber, parent_nursery) = match fiber {
                FiberTree::SingleFiber { fiber, parent_nursery } => (fiber, parent_nursery),
                _ => unreachable!(),
            };
            let TearDownResult { heap, result, .. } = fiber.tear_down();

            if let Some(nursery) = parent_nursery {
                let children = self.channels.get_mut(&nursery).unwrap().as_nursery_children_mut().unwrap();
                // TODO: Turn children into map to make this less awkward.
                let index = children.iter_mut().position(|child| child.fiber == fiber_id).unwrap();
                let child = children.remove(index);
                let is_finished = children.is_empty();

                match result {
                    Ok(return_value) => self.send_to_channel(None, child.return_channel, Packet { heap, value: return_value }),
                    Err(panic_reason) => {
                        // TODO: Handle panicking parallel section.
                    },
                };

                if is_finished {
                    let (parent, _) = self.channels.remove(&nursery).unwrap().into_nursery().unwrap();

                    let (mut paused_tree, _) = self.fibers.remove(&parent).unwrap().into_parallel_section().unwrap();
                    paused_tree.as_single_fiber_mut().unwrap().complete_parallel_scope();
                    self.fibers.insert(parent, paused_tree);
                }
            }
        }
    }

    fn try_() {
        // let result = match result {
        //     Ok(return_value) => {
        //         let ok = heap.create_symbol("Ok".to_string());
        //         heap.create_list(&[ok, return_value])
        //     },
        //     Err(panic_reason) => {
        //         let err = heap.create_symbol("Err".to_string());
        //         let reason = heap.create_text(panic_reason);
        //         heap.create_list(&[err, reason])
        //     },
        // };
    }

    fn send_to_channel(&mut self, performing_fiber: Option<FiberId>, channel: ChannelId, packet: Packet) {
        match self.channels.get_mut(&channel).unwrap() {
            Channel::Internal(channel) => {
                channel.send(&mut self.fibers, performing_fiber, packet);
            },
            Channel::External(id) => {
                let id = *id;
                self.push_external_operation(id, Operation {
                    performing_fiber,
                    kind: OperationKind::Send { packet },
                })
            },
            Channel::Nursery { children, .. } => {
                info!("Nursery received packet {:?}", packet);
                let (heap, closure_to_spawn, return_channel) = match Self::parse_spawn_packet(packet) {
                    Some(it) => it,
                    None => {
                        // The nursery received an invalid message. TODO: Panic.
                        panic!("A nursery received an invalid message.");
                    }
                };
                let fiber_id = self.fiber_id_generator.generate();
                self.fibers.insert(fiber_id, FiberTree::SingleFiber { fiber: Fiber::new_for_running_closure(heap, closure_to_spawn, &[]), parent_nursery: Some(channel) });
                children.push(Child { fiber: fiber_id, return_channel });
                InternalChannel::complete_send(&mut self.fibers, performing_fiber);
            },
        }
    }
    fn parse_spawn_packet(packet: Packet) -> Option<(Heap, Pointer, ChannelId)> {
        let Packet { mut heap, value } = packet;
        let arguments: Struct = heap.get(value).data.clone().try_into().ok()?;
        
        let closure_symbol = heap.create_symbol("Closure".to_string());
        let closure_address = arguments.get(&heap, closure_symbol)?;
        let closure: Closure = heap.get(closure_address).data.clone().try_into().ok()?;
        if closure.num_args > 0 {
            return None;
        }

        let return_channel_symbol = heap.create_symbol("ReturnChannel".to_string());
        let return_channel_address = arguments.get(&heap, return_channel_symbol)?;
        let return_channel: SendPort = heap.get(return_channel_address).data.clone().try_into().ok()?;

        Some((heap, closure_address, return_channel.channel))
    }

    fn receive_from_channel(&mut self, performing_fiber: Option<FiberId>, channel: ChannelId) {
        match self.channels.get_mut(&channel).unwrap() {
            Channel::Internal(channel) => {
                channel.receive(&mut self.fibers, performing_fiber);
            },
            Channel::External(id) => {
                let id = *id;
                self.push_external_operation(id, Operation {
                    performing_fiber,
                    kind: OperationKind::Receive,
                });
            },
            Channel::Nursery { .. } => unreachable!("nurseries are only sent stuff"),
        }
    }

    fn push_external_operation(&mut self, channel: ChannelId, operation: Operation) {
        self.external_operations.entry(channel).or_default().push(operation);
    }
}

impl InternalChannel {
    fn send(&mut self, fibers: &mut HashMap<FiberId, FiberTree>, performing_fiber: Option<FiberId>, packet: Packet) {
        self.pending_sends.push_back((performing_fiber, packet));
        self.work_on_pending_operations(fibers);
    }

    fn receive(&mut self, fibers: &mut HashMap<FiberId, FiberTree>, performing_fiber: Option<FiberId>) {
        self.pending_receives.push_back(performing_fiber);
        self.work_on_pending_operations(fibers);
    }

    fn work_on_pending_operations(&mut self, fibers: &mut HashMap<FiberId, FiberTree>) {
        if self.buffer.capacity == 0 {
            while !self.pending_sends.is_empty() && !self.pending_receives.is_empty() {
                let (send_id, packet) = self.pending_sends.pop_front().unwrap();
                let receive_id = self.pending_receives.pop_front().unwrap();
                Self::complete_send(fibers, send_id);
                Self::complete_receive(fibers, receive_id, packet);
            }
        } else {
            loop {
                let mut did_perform_operation = false;

                if !self.buffer.is_full() && let Some((fiber, packet)) = self.pending_sends.pop_front() {
                    self.buffer.send(packet);
                    Self::complete_send(fibers, fiber);
                    did_perform_operation = true;
                }

                if !self.buffer.is_empty() && let Some(fiber) = self.pending_receives.pop_front() {
                    let packet = self.buffer.receive();
                    Self::complete_receive(fibers, fiber, packet);
                    did_perform_operation = true;
                }

                if !did_perform_operation {
                    break;
                }
            }
        }
    }

    fn complete_send(fibers: &mut HashMap<FiberId, FiberTree>, fiber: Option<FiberId>) {
        if let Some(fiber) = fiber {
            let fiber = fibers.get_mut(&fiber).unwrap().as_single_fiber_mut().unwrap();
            fiber.complete_send();
        }
    }
    fn complete_receive(fibers: &mut HashMap<FiberId, FiberTree>, fiber: Option<FiberId>, packet: Packet) {
        if let Some(fiber) = fiber {
            let fiber = fibers.get_mut(&fiber).unwrap().as_single_fiber_mut().unwrap();
            fiber.complete_receive(packet);
        }
    }
}

impl Channel {
    fn into_nursery(self) -> Option<(FiberId, Vec<Child>)> {
        match self {
            Channel::Nursery { parent, children } => Some((parent, children)),
            _ => None,
        }
    }
    fn as_nursery_children(&self) -> Option<&Vec<Child>> {
        match self {
            Channel::Nursery { children, .. } => Some(children),
            _ => None,
        }
    }
    fn as_nursery_children_mut(&mut self) -> Option<&mut Vec<Child>> {
        match self {
            Channel::Nursery { children, .. } => Some(children),
            _ => None,
        }
    }
}
impl FiberTree {
    fn into_single_fiber(self) -> Option<Fiber> {
        match self {
            FiberTree::SingleFiber { fiber, .. } => Some(fiber),
            _ => None
        }
    }
    fn as_single_fiber_mut(&mut self) -> Option<&mut Fiber> {
        match self {
            FiberTree::SingleFiber { fiber, .. } => Some(fiber),
            _ => None
        }
    }

    fn into_parallel_section(self) -> Option<(FiberTree, ChannelId)> {
        match self {
            FiberTree::ParallelSection { paused_tree, nursery } => Some((*paused_tree, nursery)),
            _ => None,
        }
    }
}

impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(fiber) = self.performing_fiber {
            write!(f, "{:?} ", fiber)?;
        }
        match &self.kind {
            OperationKind::Send { packet } => write!(f, "sending {:?}", packet),
            OperationKind::Receive => write!(f, "receiving"),
        }
    }
}
impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(InternalChannel { buffer, pending_sends, pending_receives }) =>
                f.debug_struct("InternalChannel")
                    .field("buffer", buffer)
                    .field("operations", 
                        &pending_sends.iter()
                            .map(|(fiber, packet)| Operation { performing_fiber: fiber.clone(), kind: OperationKind::Send { packet: packet.clone() } })
                            .chain(pending_receives.iter().map(|fiber| Operation { performing_fiber: fiber.clone(), kind: OperationKind::Receive }))
                            .collect_vec()
                    )
                    .finish(),
            Self::External(arg0) => f.debug_tuple("External").field(arg0).finish(),
            Self::Nursery { parent, children } => f.debug_struct("Nursery").field("parent", parent).field("children", children).finish(),
        }
    }
}
impl fmt::Debug for Child {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} returning to {:?}", self.fiber, self.return_channel)
    }
}
impl fmt::Debug for FiberTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SingleFiber { fiber, parent_nursery } => f.debug_struct("SingleFiber").field("status", &fiber.status()).field("parent_nursery", parent_nursery).finish(),
            Self::ParallelSection { nursery, .. } => f.debug_struct("ParallelSection").field("nursery", nursery).finish(),
        }
    }
}
impl fmt::Debug for Vm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vm").field("fibers", &self.fibers).field("channels", &self.channels).field("external_operations", &self.external_operations).finish()
    }
}
impl fmt::Debug for FiberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fiber_{:x}", self.0)
    }
}

impl From<usize> for FiberId {
    fn from(id: usize) -> Self {
        Self(id)
    }
}

#[derive(Clone)]
struct IdGenerator<T: From<usize>> {
    next_id: usize,
    _data: PhantomData<T>,
}
impl<T: From<usize>> IdGenerator<T> {
    fn start_at(id: usize) -> Self {
        Self {
            next_id: id,
            _data: Default::default(),
        }
    }
    fn generate(&mut self) -> T {
        let id = self.next_id;
        self.next_id += 1;
        id.into()
    }
}
