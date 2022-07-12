use super::{generator::generate_n_values, utils::did_need_in_closure_cause_panic};
use crate::{
    compiler::hir,
    database::Database,
    input::Input,
    vm::{
        tracer::Tracer,
        use_provider::DbUseProvider,
        value::{Closure, Value},
        Status, Vm,
    },
};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn fuzz_closure(
    db: Arc<Mutex<Database>>,
    input: &Input,
    closure: Closure,
    closure_id: &hir::Id,
    mut num_instructions: usize,
) -> ClosureFuzzResult {
    loop {
        let arguments = generate_n_values(closure.num_args);
        let result = test_closure_with_args(
            db.clone(),
            closure.clone(),
            closure_id,
            arguments.clone(),
            num_instructions,
        )
        .await;

        match result {
            TestResult::DidNotFinishRunning => {
                break;
            }
            TestResult::FinishedRunningWithoutPanicking {
                num_instructions_executed,
            } => {
                num_instructions -= num_instructions_executed;
            }
            TestResult::ArgumentsDidNotFulfillNeeds {
                num_instructions_executed,
            } => {
                // This is the fuzzer's fault.
                num_instructions -= num_instructions_executed;
            }
            TestResult::InternalPanic { message, tracer } => {
                return ClosureFuzzResult::PanickedForArguments {
                    arguments,
                    message,
                    tracer,
                }
            }
        }
    }
    ClosureFuzzResult::NoProblemFound
}

pub enum ClosureFuzzResult {
    NoProblemFound,
    PanickedForArguments {
        arguments: Vec<Value>,
        message: Value,
        tracer: Tracer,
    },
}

async fn test_closure_with_args(
    db: Arc<Mutex<Database>>,
    closure: Closure,
    closure_id: &hir::Id,
    arguments: Vec<Value>,
    num_instructions: usize,
) -> TestResult {
    let mut vm = Vm::new();

    {
        let db = db.lock().await;
        println!("Starting closure {closure:?}.");
        let use_provider = DbUseProvider { db: &db };
        vm.set_up_closure_execution(&use_provider, closure, arguments);
        vm.run(&use_provider, num_instructions);
    }

    match vm.status() {
        Status::Running => TestResult::DidNotFinishRunning,
        Status::Done => TestResult::FinishedRunningWithoutPanicking {
            num_instructions_executed: vm.num_instructions_executed,
        },
        Status::Panicked(message) => {
            // If a `needs` directly inside the tested closure was not
            // satisfied, then the panic is closure's fault, but our fault.
            let db = db.lock().await;
            let is_our_fault =
                did_need_in_closure_cause_panic(&db, &closure_id, vm.tracer.log().last().unwrap());
            if is_our_fault {
                TestResult::ArgumentsDidNotFulfillNeeds {
                    num_instructions_executed: vm.num_instructions_executed,
                }
            } else {
                TestResult::InternalPanic {
                    message,
                    tracer: vm.tracer,
                }
            }
        }
    }
}
enum TestResult {
    DidNotFinishRunning,
    FinishedRunningWithoutPanicking { num_instructions_executed: usize },
    ArgumentsDidNotFulfillNeeds { num_instructions_executed: usize },
    InternalPanic { message: Value, tracer: Tracer },
}
