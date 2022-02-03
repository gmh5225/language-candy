mod analyzer;
mod builtin_functions;
mod compiler;
mod database;
mod incremental;
mod input;
mod interpreter;
mod language_server;

use crate::compiler::ast_to_hir::AstToHir;
use crate::compiler::cst_to_ast::CstToAst;
use crate::compiler::string_to_cst::StringToCst;
use crate::interpreter::fiber::FiberStatus;
use crate::interpreter::*;
use crate::{database::Database, input::InputReference};
use language_server::CandyLanguageServer;
use log;
use lspower::{LspService, Server};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "candy", about = "The 🍭 Candy CLI.")]
enum CandyOptions {
    Run(CandyRunOptions),
    Lsp,
}

#[derive(StructOpt, Debug)]
struct CandyRunOptions {
    #[structopt(long)]
    print_cst: bool,

    #[structopt(long)]
    print_ast: bool,

    #[structopt(long)]
    print_hir: bool,

    #[structopt(long)]
    no_run: bool,

    #[structopt(parse(from_os_str))]
    file: PathBuf,
}

#[tokio::main]
async fn main() {
    match CandyOptions::from_args() {
        CandyOptions::Run(options) => run(options),
        CandyOptions::Lsp => lsp().await,
    }
}

fn run(options: CandyRunOptions) {
    init_logger(TerminalMode::Mixed);
    let path_string = options.file.to_string_lossy();
    log::debug!("Running `{}`.\n", path_string);

    let input_reference = InputReference::File(options.file.to_owned());
    let db = Database::default();

    log::info!("Parsing string to CST…");
    let (cst, errors) = db
        .cst_raw(input_reference.clone())
        .unwrap_or_else(|| panic!("File `{}` not found.", path_string));
    if options.print_cst {
        log::info!("CST: {:#?}", cst);
    }
    if !errors.is_empty() {
        log::error!(
            "Errors occurred while parsing string to CST…:\n{:#?}",
            errors
        );
        return;
    }

    log::info!("Lowering CST to AST…");
    let (asts, _, errors) = db
        .ast_raw(input_reference.clone())
        .unwrap_or_else(|| panic!("File `{}` not found.", path_string));
    if options.print_ast {
        log::info!("AST: {:#?}", asts);
    }
    if !errors.is_empty() {
        log::error!("Errors occurred while lowering CST to AST:\n{:#?}", errors);
        return;
    }

    log::info!("Compiling AST to HIR…");
    let (lambda, _, errors) = db
        .hir_raw(input_reference.clone())
        .unwrap_or_else(|| panic!("File `{}` not found.", path_string));
    if options.print_hir {
        log::info!("HIR: {:#?}", lambda);
    }
    if !errors.is_empty() {
        log::error!("Errors occurred while lowering AST to HIR:\n{:#?}", errors);
        return;
    }

    // let reports = analyze((*lambda).clone());
    // for report in reports {
    //     log::error!("Report: {:?}", report);
    // }

    if !options.no_run {
        log::info!("Executing code…");
        let mut fiber = fiber::Fiber::new(lambda.as_ref().clone());
        fiber.run();
        match fiber.status() {
            FiberStatus::Running => log::info!("Fiber is still running."),
            FiberStatus::Done(value) => log::info!("Fiber is done: {:#?}", value),
            FiberStatus::Panicked(value) => log::error!("Fiber panicked: {:#?}", value),
        }
    }
}

async fn lsp() {
    init_logger(TerminalMode::Stderr);
    log::info!("Starting language server…");
    let (service, messages) = LspService::new(|client| CandyLanguageServer::from_client(client));
    Server::new(tokio::io::stdin(), tokio::io::stdout())
        .interleave(messages)
        .serve(service)
        .await;
}

fn init_logger(terminal_mode: TerminalMode) {
    TermLogger::init(
        LevelFilter::Error,
        Config::default(),
        terminal_mode,
        ColorChoice::Auto,
    )
    .unwrap();
}
