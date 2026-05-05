use anyhow::Result;
use fabro_api::types;
use fabro_util::printer::Printer;

use crate::args::{SystemRepairArgs, SystemRepairCommand, SystemRepairRunsArgs};
use crate::command_context::CommandContext;
use crate::shared::print_json_pretty;

pub(super) async fn repair_command(
    args: &SystemRepairArgs,
    base_ctx: &CommandContext,
) -> Result<()> {
    match &args.command {
        SystemRepairCommand::Runs(args) => repair_runs_command(args, base_ctx).await,
    }
}

async fn repair_runs_command(args: &SystemRepairRunsArgs, base_ctx: &CommandContext) -> Result<()> {
    let ctx = base_ctx.with_connection(&args.connection)?;
    let server = ctx.server().await?;
    let response = server.get_system_repair_runs().await?;
    repair_runs_from(&response, ctx.json_output(), ctx.printer())
}

fn repair_runs_from(
    response: &types::SystemRepairRunsResponse,
    json_output: bool,
    printer: Printer,
) -> Result<()> {
    if json_output {
        print_json_pretty(response)?;
        return Ok(());
    }

    if response.runs.is_empty() {
        fabro_util::printout!(printer, "No run repair issues found.");
        return Ok(());
    }

    fabro_util::printout!(printer, "Unreadable runs:");
    for run in &response.runs {
        fabro_util::printout!(
            printer,
            "  {}  {}  {}",
            run.run_id,
            run.created_at.to_rfc3339(),
            run.error,
        );
    }

    fabro_util::printout!(printer, "");
    fabro_util::printout!(printer, "Delete with:");
    for run in &response.runs {
        fabro_util::printout!(printer, "  fabro rm --force {}", run.run_id);
    }
    Ok(())
}
