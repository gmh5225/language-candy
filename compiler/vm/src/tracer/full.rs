use super::{FiberEvent, FiberTracer, Tracer, VmEvent};
use crate::{
    channel::ChannelId,
    fiber::FiberId,
    heap::{Heap, Pointer},
};
use itertools::Itertools;
use std::{fmt, time::Instant};

/// A full tracer that saves all events that occur with timestamps.
#[derive(Clone, Default)]
pub struct FullTracer {
    pub events: Vec<TimedEvent>,
    pub heap: Heap,
}
#[derive(Clone)]
pub struct TimedEvent {
    pub when: Instant,
    pub event: StoredVmEvent,
}

#[derive(Clone)]
pub enum StoredVmEvent {
    FiberCreated {
        fiber: FiberId,
    },
    FiberDone {
        fiber: FiberId,
    },
    FiberPanicked {
        fiber: FiberId,
        panicked_child: Option<FiberId>,
    },
    FiberCanceled {
        fiber: FiberId,
    },
    FiberExecutionStarted {
        fiber: FiberId,
    },
    FiberExecutionEnded {
        fiber: FiberId,
    },
    ChannelCreated {
        channel: ChannelId,
    },
    InFiber {
        fiber: FiberId,
        event: StoredFiberEvent,
    },
}
#[derive(Clone)]
pub enum StoredFiberEvent {
    ValueEvaluated {
        expression: Pointer,
        value: Pointer,
    },
    FoundFuzzableClosure {
        definition: Pointer,
        closure: Pointer,
    },
    CallStarted {
        call_site: Pointer,
        callee: Pointer,
        arguments: Vec<Pointer>,
        responsible: Pointer,
    },
    CallEnded {
        return_value: Pointer,
    },
}

struct FullFiberTracer {}

impl Tracer for FullTracer {
    fn add(&mut self, event: VmEvent) {
        let event = TimedEvent {
            when: Instant::now(),
            event: self.map_vm_event(event),
        };
        self.events.push(event);
    }

    type ForFiber;

    fn fiber_created(&mut self, fiber: FiberId) {
        todo!()
    }

    fn fiber_done(&mut self, fiber: FiberId) {
        todo!()
    }

    fn fiber_panicked(&mut self, fiber: FiberId, panicked_child: Option<FiberId>) {
        todo!()
    }

    fn fiber_canceled(&mut self, fiber: FiberId) {
        todo!()
    }

    fn fiber_execution_started(&mut self, fiber: FiberId) {
        todo!()
    }

    fn fiber_execution_ended(&mut self, fiber: FiberId) {
        todo!()
    }

    fn channel_created(&mut self, channel: ChannelId) {
        todo!()
    }

    fn tracer_for_fiber(&mut self, fiber: FiberId) -> super::FiberTracer {
        todo!()
    }

    fn fiber_exited(&mut self, fiber_tracer: Self::ForFiber) {
        todo!()
    }
}

impl FiberTracer for FullFiberTracer {
    fn value_evaluated(&mut self, expression: Pointer, value: Pointer, heap: &mut Heap) {
        todo!()
    }

    fn found_fuzzable_closure(&mut self, definition: Pointer, closure: Pointer, heap: &mut Heap) {
        todo!()
    }

    fn call_started(
        &mut self,
        call_site: Pointer,
        callee: Pointer,
        args: Vec<Pointer>,
        responsible: Pointer,
        heap: &mut Heap,
    ) {
        todo!()
    }

    fn call_ended(&mut self, return_value: Pointer, heap: &mut Heap) {
        todo!()
    }
}

impl FullTracer {
    fn import_from_heap(&mut self, address: Pointer, heap: &Heap) -> Pointer {
        heap.clone_single_to_other_heap(&mut self.heap, address)
    }

    fn map_vm_event(&mut self, event: VmEvent) -> StoredVmEvent {
        match event {
            VmEvent::FiberCreated { fiber } => StoredVmEvent::FiberCreated { fiber },
            VmEvent::FiberDone { fiber } => StoredVmEvent::FiberDone { fiber },
            VmEvent::FiberPanicked {
                fiber,
                panicked_child,
            } => StoredVmEvent::FiberPanicked {
                fiber,
                panicked_child,
            },
            VmEvent::FiberCanceled { fiber } => StoredVmEvent::FiberCanceled { fiber },
            VmEvent::FiberExecutionStarted { fiber } => {
                StoredVmEvent::FiberExecutionStarted { fiber }
            }
            VmEvent::FiberExecutionEnded { fiber } => StoredVmEvent::FiberExecutionEnded { fiber },
            VmEvent::ChannelCreated { channel } => StoredVmEvent::ChannelCreated { channel },
            VmEvent::InFiber { fiber, event } => StoredVmEvent::InFiber {
                fiber,
                event: self.map_fiber_event(event),
            },
        }
    }
    fn map_fiber_event(&mut self, event: FiberEvent) -> StoredFiberEvent {
        match event {
            FiberEvent::ValueEvaluated {
                expression,
                value,
                heap,
            } => {
                let expression = self.import_from_heap(expression, heap);
                let value = self.import_from_heap(value, heap);
                StoredFiberEvent::ValueEvaluated { expression, value }
            }
            FiberEvent::FoundFuzzableClosure {
                definition,
                closure,
                heap,
            } => {
                let definition = self.import_from_heap(definition, heap);
                let closure = self.import_from_heap(closure, heap);
                StoredFiberEvent::FoundFuzzableClosure {
                    definition,
                    closure,
                }
            }
            FiberEvent::CallStarted {
                call_site,
                callee,
                arguments,
                responsible,
                heap,
            } => {
                let call_site = self.import_from_heap(call_site, heap);
                let callee = self.import_from_heap(callee, heap);
                let arguments = arguments
                    .into_iter()
                    .map(|arg| self.import_from_heap(arg, heap))
                    .collect();
                let responsible = self.import_from_heap(responsible, heap);
                StoredFiberEvent::CallStarted {
                    call_site,
                    callee,
                    arguments,
                    responsible,
                }
            }
            FiberEvent::CallEnded { return_value, heap } => {
                let return_value = self.import_from_heap(return_value, heap);
                StoredFiberEvent::CallEnded { return_value }
            }
        }
    }
}

impl fmt::Debug for FullTracer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = self.events.first().map(|event| event.when);
        for event in &self.events {
            writeln!(
                f,
                "{:?} µs: {}",
                event.when.duration_since(start.unwrap()).as_micros(),
                match &event.event {
                    StoredVmEvent::FiberCreated { fiber } => format!("{fiber:?}: created"),
                    StoredVmEvent::FiberDone { fiber } => format!("{fiber:?}: done"),
                    StoredVmEvent::FiberPanicked {
                        fiber,
                        panicked_child,
                    } => format!(
                        "{fiber:?}: panicked{}",
                        if let Some(child) = panicked_child {
                            format!(" because child {child:?} panicked")
                        } else {
                            "".to_string()
                        }
                    ),
                    StoredVmEvent::FiberCanceled { fiber } => format!("{fiber:?}: canceled"),
                    StoredVmEvent::FiberExecutionStarted { fiber } =>
                        format!("{fiber:?}: execution started"),
                    StoredVmEvent::FiberExecutionEnded { fiber } =>
                        format!("{fiber:?}: execution ended"),
                    StoredVmEvent::ChannelCreated { channel } => format!("{channel:?}: created"),
                    StoredVmEvent::InFiber { fiber, event } => format!(
                        "{fiber:?}: {}",
                        match event {
                            StoredFiberEvent::ValueEvaluated { expression, value } =>
                                format!("value {expression} is {}", value.format(&self.heap)),
                            StoredFiberEvent::FoundFuzzableClosure { definition, .. } =>
                                format!("found fuzzable closure {definition}"),
                            StoredFiberEvent::CallStarted {
                                call_site,
                                callee,
                                arguments,
                                responsible,
                            } => format!(
                                "call started: {} {} (call site {}, {} is responsible)",
                                callee.format(&self.heap),
                                arguments.iter().map(|arg| arg.format(&self.heap)).join(" "),
                                self.heap.get_hir_id(*call_site),
                                self.heap.get_hir_id(*responsible),
                            ),
                            StoredFiberEvent::CallEnded { return_value } =>
                                format!("call ended: {}", return_value.format(&self.heap)),
                        },
                    ),
                },
            )?;
        }
        Ok(())
    }
}
