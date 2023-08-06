use super::{insights::Insight, static_panics::StaticPanicsOfMir};
use crate::{
    database::Database, features_candy::analyzer::insights::ErrorDiagnostic,
    server::AnalyzerClient, utils::LspPositionConversion,
};
use candy_frontend::{
    ast_to_hir::AstToHir, mir_optimize::OptimizeMir, module::Module, TracingConfig, TracingMode,
};
use candy_fuzzer::{FuzzablesFinder, Fuzzer, Status};
use candy_vm::{
    heap::{DisplayWithSymbolTable, Heap},
    lir::Lir,
    mir_to_lir::compile_lir,
    tracer::{evaluated_values::EvaluatedValuesTracer, stack_trace::StackTracer},
    Panic, Vm, VmPanicked, VmReturned,
};
use extension_trait::extension_trait;
use itertools::Itertools;
use lsp_types::Diagnostic;
use rand::{prelude::SliceRandom, thread_rng};
use std::rc::Rc;
use tracing::info;

/// A hints finder is responsible for finding hints for a single module.
pub struct ModuleAnalyzer {
    module: Module,
    state: Option<State>, // only None during state transition
}
enum State {
    Initial,
    /// First, we run the module with tracing of evaluated expressions enabled.
    /// This enables us to show hints for constants.
    EvaluateConstants {
        static_panics: Vec<Panic>,
        vm: Vm<Lir, (StackTracer, EvaluatedValuesTracer)>,
    },
    /// Next, we run the module again to finds fuzzable functions. This time, we
    /// disable tracing of evaluated expressions, but we enable registration of
    /// fuzzable functions. Thus, the found functions to fuzz have the most
    /// efficient LIR possible.
    FindFuzzables {
        static_panics: Vec<Panic>,
        heap_for_constants: Heap,
        stack_tracer: StackTracer,
        evaluated_values: EvaluatedValuesTracer,
        lir: Rc<Lir>,
        vm: Vm<Rc<Lir>, FuzzablesFinder>,
    },
    /// Then, the functions are actually fuzzed.
    Fuzz {
        lir: Rc<Lir>,
        static_panics: Vec<Panic>,
        heap_for_constants: Heap,
        stack_tracer: StackTracer,
        evaluated_values: EvaluatedValuesTracer,
        heap_for_fuzzables: Heap,
        fuzzers: Vec<Fuzzer>,
    },
}

impl ModuleAnalyzer {
    pub fn for_module(module: Module) -> Self {
        Self {
            module,
            state: Some(State::Initial),
        }
    }
    pub fn module_changed(&mut self) {
        // PERF: Save some incremental state.
        self.state = Some(State::Initial);
    }

    pub async fn run(&mut self, db: &Database, client: &AnalyzerClient) {
        let state = self.state.take().unwrap();
        let state = self.update_state(db, client, state).await;
        self.state = Some(state);
    }
    async fn update_state(&self, db: &Database, client: &AnalyzerClient, state: State) -> State {
        match state {
            State::Initial => {
                client
                    .update_status(Some(format!("Compiling {}", self.module)))
                    .await;

                let (mir, _, _) = db
                    .optimized_mir(
                        self.module.clone(),
                        TracingConfig {
                            register_fuzzables: TracingMode::OnlyCurrent,
                            calls: TracingMode::Off,
                            evaluated_expressions: TracingMode::Off,
                        },
                    )
                    .unwrap();
                let mut mir = (*mir).clone();
                let mut static_panics = mir.static_panics();
                static_panics.retain(|panic| panic.responsible.module == self.module);

                let tracing = TracingConfig {
                    register_fuzzables: TracingMode::Off,
                    calls: TracingMode::Off,
                    evaluated_expressions: TracingMode::OnlyCurrent,
                };
                let (lir, _) = compile_lir(db, self.module.clone(), tracing);

                let tracer = (
                    StackTracer::default(),
                    EvaluatedValuesTracer::new(self.module.clone()),
                );
                let vm = Vm::for_module(lir, tracer);

                State::EvaluateConstants { static_panics, vm }
            }
            State::EvaluateConstants { static_panics, vm } => {
                client
                    .update_status(Some(format!("Evaluating {}", self.module)))
                    .await;

                let (heap, tracer) = match vm.run_n(500) {
                    candy_vm::StateAfterRun::Running(vm) => {
                        return State::EvaluateConstants { static_panics, vm }
                    }
                    candy_vm::StateAfterRun::CallingHandle(_) => unreachable!(),
                    candy_vm::StateAfterRun::Returned(VmReturned { heap, tracer, .. }) => {
                        (heap, tracer)
                    }
                    candy_vm::StateAfterRun::Panicked(VmPanicked { heap, tracer, .. }) => {
                        (heap, tracer)
                    }
                };
                let (stack_tracer, evaluated_values) = tracer;

                let tracing = TracingConfig {
                    register_fuzzables: TracingMode::OnlyCurrent,
                    calls: TracingMode::Off,
                    evaluated_expressions: TracingMode::Off,
                };
                let (lir, _) = compile_lir(db, self.module.clone(), tracing);
                let lir = Rc::new(lir);

                let vm = Vm::for_module(lir.clone(), FuzzablesFinder::default());
                State::FindFuzzables {
                    static_panics,
                    heap_for_constants: heap,
                    stack_tracer,
                    evaluated_values,
                    lir,
                    vm,
                }
            }
            State::FindFuzzables {
                static_panics,
                heap_for_constants,
                stack_tracer,
                evaluated_values,
                lir,
                vm,
            } => {
                client
                    .update_status(Some(format!("Evaluating {}", self.module)))
                    .await;

                let (heap, tracer) = match vm.run_n(500) {
                    candy_vm::StateAfterRun::Running(vm) => {
                        return State::FindFuzzables {
                            static_panics,
                            heap_for_constants,
                            stack_tracer,
                            evaluated_values,
                            lir,
                            vm,
                        }
                    }
                    candy_vm::StateAfterRun::CallingHandle(_) => unreachable!(),
                    candy_vm::StateAfterRun::Returned(VmReturned { heap, tracer, .. }) => {
                        (heap, tracer)
                    }
                    candy_vm::StateAfterRun::Panicked(VmPanicked { heap, tracer, .. }) => {
                        (heap, tracer)
                    }
                };

                let fuzzers = tracer
                    .fuzzables
                    .iter()
                    .map(|(id, function)| Fuzzer::new(lir.clone(), *function, id.clone()))
                    .collect();
                State::Fuzz {
                    lir,
                    static_panics,
                    heap_for_constants,
                    stack_tracer,
                    evaluated_values,
                    heap_for_fuzzables: heap,
                    fuzzers,
                }
            }
            State::Fuzz {
                lir,
                static_panics,
                heap_for_constants,
                stack_tracer,
                evaluated_values,
                heap_for_fuzzables,
                mut fuzzers,
            } => {
                let mut running_fuzzers = fuzzers
                    .iter_mut()
                    .filter(|fuzzer| matches!(fuzzer.status(), Status::StillFuzzing { .. }))
                    .collect_vec();
                let Some(fuzzer) = running_fuzzers.choose_mut(&mut thread_rng()) else {
                    client.update_status(None).await;
                    return State::Fuzz {
                        lir,
                        static_panics,
                        heap_for_constants,
                        stack_tracer,
                        evaluated_values,
                        heap_for_fuzzables,
                        fuzzers,
                    };
                };

                client
                    .update_status(Some(format!("Fuzzing {}", fuzzer.function_id)))
                    .await;

                fuzzer.run(500);

                State::Fuzz {
                    lir,
                    static_panics,
                    heap_for_constants,
                    stack_tracer,
                    evaluated_values,
                    heap_for_fuzzables,
                    fuzzers,
                }
            }
        }
    }

    pub fn insights(&self, db: &Database) -> Vec<Insight> {
        let mut insights = vec![];

        match self.state.as_ref().unwrap() {
            State::Initial => {}
            State::EvaluateConstants { static_panics, .. } => {
                // TODO: Show incremental constant evaluation hints.
                insights.extend(static_panics.to_insights(db, &self.module));
            }
            State::FindFuzzables {
                static_panics,
                evaluated_values,
                vm,
                ..
            } => {
                insights.extend(static_panics.to_insights(db, &self.module));
                insights.extend(evaluated_values.values().iter().flat_map(|(id, value)| {
                    Insight::for_value(db, &vm.lir.symbol_table, id.clone(), *value)
                }));
            }
            State::Fuzz {
                lir,
                static_panics,
                evaluated_values,
                fuzzers,
                ..
            } => {
                insights.extend(static_panics.to_insights(db, &self.module));
                let symbol_table = &lir.symbol_table;
                insights.extend(evaluated_values.values().iter().flat_map(|(id, value)| {
                    Insight::for_value(db, symbol_table, id.clone(), *value)
                }));

                for fuzzer in fuzzers {
                    insights.append(&mut Insight::for_fuzzer_status(db, fuzzer));

                    let Status::FoundPanic { input, panic, .. } = fuzzer.status() else {
                        continue;
                    };

                    let id = fuzzer.function_id.clone();
                    if !id.is_same_module_and_any_parent_of(&panic.responsible) {
                        // The function panics internally for an input, but it's
                        // the fault of another function that's called
                        // internally.
                        // TODO: The fuzz case should instead be highlighted in
                        // the used function directly. We don't do that right
                        // now because we assume the fuzzer will find the panic
                        // when fuzzing the faulty function, but we should save
                        // the panicking case (or something like that) in the
                        // future.
                        continue;
                    }
                    if db.hir_to_cst_id(id.clone()).is_none() {
                        panic!(
                            "It looks like the generated code {} is at fault for a panic.",
                            panic.responsible,
                        );
                    }

                    // TODO: In the future, re-run only the failing case with
                    // tracing enabled and also show the arguments to the failing
                    // function in the hint.
                    let call_span = db
                        .hir_id_to_display_span(panic.responsible.clone())
                        .unwrap();
                    insights.push(Insight::Diagnostic(Diagnostic::error(
                        db.range_to_lsp_range(self.module.clone(), call_span),
                        format!(
                            "For `{} {}`, this call panics: {}",
                            fuzzer.function_id.function_name(),
                            input
                                .arguments
                                .iter()
                                .map(|argument| DisplayWithSymbolTable::to_string(
                                    argument,
                                    symbol_table,
                                ))
                                .join(" "),
                            panic.reason,
                        ),
                    )));
                }
            }
        }

        info!("Insights: {insights:?}");

        insights
    }
}

#[extension_trait]
pub impl StaticPanics for Vec<Panic> {
    fn to_insights(&self, db: &Database, module: &Module) -> Vec<Insight> {
        self.iter()
            .map(|panic| Insight::for_static_panic(db, module.clone(), panic))
            .collect_vec()
    }
}
