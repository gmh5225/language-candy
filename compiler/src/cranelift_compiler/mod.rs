use std::{error::Error, sync};

use cranelift::prelude::*;
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::compiler::{self, hir::Body, mir::Expression};

pub(crate) fn compile(optimized_mir: sync::Arc<compiler::mir::Mir>) -> Result<(), Box<dyn Error>> {
    let mut shared_builder = settings::builder();
    shared_builder.enable("is_pic").unwrap();
    let shared_flags = settings::Flags::new(shared_builder);

    let target = target_lexicon::DefaultToHost::default();
    let isa_builder = isa::lookup(target.0).unwrap();
    let isa = isa_builder.finish(shared_flags).unwrap();
    let call_conv = isa.default_call_conv();

    let obj_builder =
        ObjectBuilder::new(isa, "main", cranelift_module::default_libcall_names()).unwrap();
    let mut obj_module = ObjectModule::new(obj_builder);

    for (id, expr) in optimized_mir.body.iter() {
        // Compile expressions
        match expr {
            compiler::mir::Expression::Int(_) => todo!(),
            compiler::mir::Expression::Text(_) => todo!(),
            compiler::mir::Expression::Symbol(symbol) => {
                dbg!(symbol);
            }
            compiler::mir::Expression::Builtin(_) => todo!(),
            compiler::mir::Expression::List(_) => todo!(),
            compiler::mir::Expression::Struct(_) => todo!(),
            compiler::mir::Expression::Reference(_) => todo!(),
            compiler::mir::Expression::HirId(_) => todo!(),
            compiler::mir::Expression::Lambda {
                parameters,
                responsible_parameter,
                body,
            } => {
                dbg!("Encountered Lambda");
                compile_lambda(&expr)
            }
            compiler::mir::Expression::Parameter => todo!(),
            compiler::mir::Expression::Call {
                function,
                arguments,
                responsible,
            } => todo!(),
            compiler::mir::Expression::UseModule {
                current_module,
                relative_path,
                responsible,
            } => todo!(),
            compiler::mir::Expression::Panic {
                reason,
                responsible,
            } => todo!(),
            compiler::mir::Expression::Multiple(_) => todo!(),
            compiler::mir::Expression::ModuleStarts { module } => { //purposefully ignored
            }
            compiler::mir::Expression::ModuleEnds => todo!(),
            compiler::mir::Expression::TraceCallStarts {
                hir_call,
                function,
                arguments,
                responsible,
            } => todo!(),
            compiler::mir::Expression::TraceCallEnds { return_value } => todo!(),
            compiler::mir::Expression::TraceExpressionEvaluated {
                hir_expression,
                value,
            } => todo!(),
            compiler::mir::Expression::TraceFoundFuzzableClosure {
                hir_definition,
                closure,
            } => todo!(),
        }
    }
    Ok(())
}

fn compile_lambda(lambda: &Expression) {
    assert!(matches!(lambda, &Expression::Lambda { .. }));
}
