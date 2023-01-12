use std::{error::Error, sync::Arc};

use cranelift::codegen::ir::Value;
use cranelift::{
    codegen::ir::{Function, UserFuncName},
    prelude::*,
};
use cranelift_module::{DataContext, DataId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;

use crate::compiler::mir::Id;
use crate::compiler::{
    self,
    hir::Body,
    mir::{Expression, Mir},
};

pub struct CodeGen {
    program: Arc<Mir>,
    symbols: HashMap<Id, DataId>,
    values: HashMap<Id, Value>,
    module_data: HashMap<Id, DataId>,
}

impl CodeGen {
    pub fn new(program: Arc<Mir>) -> Self {
        Self {
            program,
            symbols: HashMap::new(),
            values: HashMap::new(),
            module_data: HashMap::new(),
        }
    }

    pub(crate) fn compile(&mut self) -> Result<(), Box<dyn Error>> {
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

        let mut sig = Signature::new(call_conv);
        sig.returns.push(AbiParam::new(types::I32));
        let candy_rt_main_id = obj_module
            .declare_function("candy_rt_main", Linkage::Export, &sig)
            .unwrap();

        let mut candy_rt_main = Function::with_name_signature(UserFuncName::user(0, 0), sig);
        let mut candy_rt_main_ctx = FunctionBuilderContext::new();
        let mut candy_rt_main_builder =
            FunctionBuilder::new(&mut candy_rt_main, &mut candy_rt_main_ctx);

        let mut data_ctx = DataContext::new();

        for (id, expr) in self.program.body.iter() {
            // Compile expressions
            match expr {
                compiler::mir::Expression::Int(int) => {
                    // This should probably more accurately be i128
                    let val = candy_rt_main_builder
                        .ins()
                        .iconst::<i64>(types::I64, int.try_into().unwrap());
                    self.values.insert(id, val);
                }
                compiler::mir::Expression::Text(text) => {
                    let data = obj_module
                        .declare_data(text, Linkage::Local, false, false)
                        .unwrap();
                    data_ctx.define(text.clone().into_bytes().into_boxed_slice());
                    obj_module.define_data(data, &data_ctx).unwrap();
                    data_ctx.clear();
                    self.module_data.insert(id, data);
                }
                compiler::mir::Expression::Symbol(symbol) => {
                    dbg!(symbol);
                    let data = obj_module
                        .declare_data(symbol, Linkage::Local, false, false)
                        .unwrap();
                    data_ctx.define(symbol.clone().into_bytes().into_boxed_slice());
                    obj_module.define_data(data, &data_ctx).unwrap();
                    data_ctx.clear();
                    self.symbols.insert(id, data);
                }
                compiler::mir::Expression::Builtin(_) => todo!(),
                compiler::mir::Expression::List(_) => todo!(),
                compiler::mir::Expression::Struct(struct_) => {
                    dbg!("Struct defined here");
                    dbg!(struct_);
                }
                compiler::mir::Expression::Reference(reference) => {
                    dbg!("Reference to", reference);
                }
                compiler::mir::Expression::HirId(_) => todo!(),
                compiler::mir::Expression::Lambda {
                    parameters,
                    responsible_parameter,
                    body,
                } => {
                    dbg!("Encountered Lambda");
                    self.compile_lambda(expr);
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
                compiler::mir::Expression::ModuleEnds => {
                    // Purposefully ignored (for now)
                    // Probably want to finalize exports map here
                }
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

    fn compile_lambda(&self, lambda: &Expression) {
        assert!(matches!(lambda, &Expression::Lambda { .. }));
    }
}
