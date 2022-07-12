use crate::{
    compiler::hir,
    database::Database,
    fuzzer::closure_fuzzer::{fuzz_closure, ClosureFuzzResult},
    input::Input,
    vm::{
        tracer::Tracer,
        use_provider::DbUseProvider,
        value::{Closure, Value},
        Status, TearDownResult, Vm,
    },
};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn fuzz_input(db: Arc<Mutex<Database>>, input: Input) -> Vec<ClosurePanic> {
    let mut vm = {
        let mut vm = Vm::new();
        let db = db.lock().await;
        let module_closure = Closure::of_input(&db, input.clone()).unwrap();
        let use_provider = DbUseProvider { db: &db };
        vm.set_up_module_closure_execution(&use_provider, module_closure);
        vm.run(&use_provider, 1000);
        vm
    };

    match vm.status() {
        Status::Running => {
            log::warn!("The VM didn't finish running, so we're not fuzzing it.");
            return vec![];
        }
        Status::Done => log::debug!("The VM is done."),
        Status::Panicked(value) => {
            log::error!("The VM panicked with value {value}.");
            let db = db.lock().await;
            log::error!("{}", vm.tracer.format_stack_trace(&db, input.clone()));
            return vec![];
        }
    }
    let TearDownResult {
        fuzzable_closures, ..
    } = vm.tear_down_module_closure_execution();

    log::info!(
        "Now, the fuzzing begins. So far, we have {} closures to fuzz.",
        fuzzable_closures.len()
    );

    let mut panics = vec![];
    for (id, closure) in fuzzable_closures {
        match fuzz_closure(db.clone(), &input, closure.clone(), &id, 1000).await {
            ClosureFuzzResult::NoProblemFound => {}
            ClosureFuzzResult::PanickedForArguments {
                arguments,
                message,
                tracer,
            } => panics.push(ClosurePanic {
                closure,
                closure_id: id,
                arguments,
                message,
                tracer,
            }),
        }
    }
    panics
}

pub struct ClosurePanic {
    pub closure: Closure,
    pub closure_id: hir::Id,
    pub arguments: Vec<Value>,
    pub message: Value,
    pub tracer: Tracer,
}
